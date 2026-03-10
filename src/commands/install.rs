#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::io::ErrorKind;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

use crate::commands::CommandReport;
use crate::commands::moon_stop;
use crate::moon::config::load_context_policy_if_explicit_env;
use crate::openclaw::config::{
    ConfigPatchOptions, apply_config_patches, ensure_plugin_enabled, ensure_plugin_install_record,
    read_config_value, write_config_atomic,
};
use crate::openclaw::paths::resolve_paths;
use crate::openclaw::plugin_install;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub force: bool,
    pub dry_run: bool,
    pub apply: bool,
}

pub fn run(opts: &InstallOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("install");

    report.detail("preflight: stopping watcher daemon and clearing lock".to_string());
    report.merge(moon_stop::run()?);

    let plugin = plugin_install::install_plugin(&paths, opts.dry_run)?;
    report.detail(format!("plugin_dir={}", plugin.path));
    report.detail(format!("plugin_changed={}", plugin.changed));

    let mut cfg = read_config_value(&paths)?;
    let context_policy = load_context_policy_if_explicit_env()?;
    if let Some(policy) = &context_policy {
        report.detail(format!(
            "context.policy=window_mode={:?} prune_mode={:?} compaction_authority={:?}",
            policy.window_mode, policy.prune_mode, policy.compaction_authority
        ));
    } else {
        report.detail(
            "context.policy=legacy (no explicit MOON_CONFIG_PATH/MOON_HOME context section)"
                .to_string(),
        );
    }

    let patch = apply_config_patches(
        &mut cfg,
        &ConfigPatchOptions { force: opts.force },
        &paths.plugin_id,
        context_policy.as_ref(),
    );

    let plugin_patch = ensure_plugin_enabled(&mut cfg, &paths.plugin_id);
    let install_record_patch =
        ensure_plugin_install_record(&mut cfg, &paths.plugin_id, &paths.plugin_dir);

    for key in patch.inserted_paths {
        report.detail(format!("inserted {key}"));
    }
    for key in patch.forced_paths {
        report.detail(format!("forced {key}"));
    }
    for key in patch.removed_paths {
        report.detail(format!("removed {key}"));
    }
    for key in plugin_patch.inserted_paths {
        report.detail(format!("inserted {key}"));
    }
    for key in plugin_patch.forced_paths {
        report.detail(format!("forced {key}"));
    }
    for key in install_record_patch.inserted_paths {
        report.detail(format!("inserted {key}"));
    }
    for key in install_record_patch.forced_paths {
        report.detail(format!("forced {key}"));
    }

    let changed =
        patch.changed || plugin_patch.changed || install_record_patch.changed || plugin.changed;
    if changed && opts.apply && !opts.dry_run {
        let path_written = write_config_atomic(&paths, &cfg)?;
        report.detail(format!("updated config: {path_written}"));
    } else if changed && (opts.dry_run || !opts.apply) {
        report.detail("config changes planned but not applied".to_string());
    } else {
        report.detail("config already satisfied".to_string());
    }

    if let Err(err) = ensure_default_autostart(opts, &mut report) {
        report.issue(format!("autostart setup failed: {err:#}"));
    }

    Ok(report)
}

#[cfg(not(target_os = "macos"))]
fn ensure_default_autostart(opts: &InstallOptions, report: &mut CommandReport) -> Result<()> {
    let _ = opts;
    report.detail("autostart=skipped reason=unsupported_platform".to_string());
    Ok(())
}

#[cfg(target_os = "macos")]
const LAUNCHD_LABEL: &str = "com.moon.watch";
#[cfg(target_os = "macos")]
const LAUNCHD_PLIST_NAME: &str = "com.moon.watch.plist";
#[cfg(target_os = "macos")]
const CAFFEINATE_PATH: &str = "/usr/bin/caffeinate";

#[cfg(target_os = "macos")]
fn ensure_default_autostart(opts: &InstallOptions, report: &mut CommandReport) -> Result<()> {
    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    report.detail(format!("autostart.provider=launchd label={LAUNCHD_LABEL}"));

    if is_dev_build_path(&current_exe) {
        report.detail(format!(
            "autostart.launchd=skipped reason=development_binary path={}",
            current_exe.display()
        ));
        report.detail(
            "autostart.hint=run `cargo install --path .` then rerun `moon install` from installed binary"
                .to_string(),
        );
        return Ok(());
    }

    let moon_paths = crate::moon::paths::resolve_paths()?;
    let home_dir = dirs::home_dir().context("HOME directory could not be resolved")?;
    let launch_agents_dir = home_dir.join("Library").join("LaunchAgents");
    let plist_path = launch_agents_dir.join(LAUNCHD_PLIST_NAME);
    let stdout_path = moon_paths.logs_dir.join("launchd.stdout.log");
    let stderr_path = moon_paths.logs_dir.join("launchd.stderr.log");
    let working_dir =
        env::current_dir().context("failed to resolve current working directory for launchd")?;
    let moon_config_path = crate::moon::config::resolve_config_path();
    let path_value = default_launchd_path(&home_dir, current_exe.parent());
    let plist_payload = render_launchd_plist(
        LAUNCHD_LABEL,
        &current_exe,
        &working_dir,
        &moon_paths.moon_home,
        &moon_paths.logs_dir,
        &stdout_path,
        &stderr_path,
        &home_dir,
        &path_value,
        moon_config_path.as_deref(),
    );

    report.detail(format!(
        "autostart.launchd.binary={}",
        current_exe.display()
    ));
    report.detail(format!(
        "autostart.launchd.wrapper={} -i -s",
        CAFFEINATE_PATH
    ));
    report.detail(format!("autostart.launchd.plist={}", plist_path.display()));
    if opts.dry_run {
        report.detail("autostart.launchd.mode=dry-run (no launchctl changes)".to_string());
        return Ok(());
    }

    fs::create_dir_all(&launch_agents_dir)
        .with_context(|| format!("failed to create {}", launch_agents_dir.display()))?;
    fs::create_dir_all(&moon_paths.logs_dir)
        .with_context(|| format!("failed to create {}", moon_paths.logs_dir.display()))?;

    let plist_changed = match fs::read_to_string(&plist_path) {
        Ok(existing) => existing != plist_payload,
        Err(err) if err.kind() == ErrorKind::NotFound => true,
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", plist_path.display()));
        }
    };
    if plist_changed {
        fs::write(&plist_path, plist_payload)
            .with_context(|| format!("failed to write {}", plist_path.display()))?;
    }
    report.detail(format!("autostart.launchd.plist_changed={plist_changed}"));

    let uid = resolve_uid()?;
    let domain = format!("gui/{uid}");
    let plist_arg = plist_path.display().to_string();
    let bootout_out = run_launchctl(["bootout", &domain, &plist_arg].as_slice())?;
    if bootout_out.status.success() {
        report.detail("autostart.launchd.bootout=ok".to_string());
    } else {
        report.detail(format!(
            "autostart.launchd.bootout=ignored ({})",
            summarize_command_failure(&bootout_out)
        ));
    }

    let bootstrap_out = run_launchctl(["bootstrap", &domain, &plist_arg].as_slice())?;
    if !bootstrap_out.status.success() {
        anyhow::bail!(
            "launchctl bootstrap failed: {}",
            summarize_command_failure(&bootstrap_out)
        );
    }
    report.detail("autostart.launchd.bootstrap=ok".to_string());

    let target = format!("{domain}/{LAUNCHD_LABEL}");
    let kickstart_out = run_launchctl(["kickstart", "-k", &target].as_slice())?;
    if !kickstart_out.status.success() {
        anyhow::bail!(
            "launchctl kickstart failed: {}",
            summarize_command_failure(&kickstart_out)
        );
    }
    report.detail("autostart.launchd.kickstart=ok".to_string());
    report.detail("autostart.launchd.enabled=true".to_string());
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_launchctl(args: &[&str]) -> Result<std::process::Output> {
    Command::new("launchctl")
        .args(args)
        .output()
        .with_context(|| format!("failed to execute launchctl {}", args.join(" ")))
}

#[cfg(target_os = "macos")]
fn summarize_command_failure(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    match output.status.code() {
        Some(code) => format!("exit code {code}"),
        None => "terminated by signal".to_string(),
    }
}

#[cfg(target_os = "macos")]
fn resolve_uid() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve user id via `id -u`")?;
    if !output.status.success() {
        anyhow::bail!("`id -u` failed: {}", summarize_command_failure(&output));
    }

    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uid.is_empty() {
        anyhow::bail!("`id -u` returned empty output");
    }
    Ok(uid)
}

#[cfg(target_os = "macos")]
fn is_dev_build_path(path: &Path) -> bool {
    let normalized = path.display().to_string();
    normalized.contains("target/debug")
        || normalized.contains("target/release")
        || normalized.contains("target\\debug")
        || normalized.contains("target\\release")
}

#[cfg(target_os = "macos")]
fn default_launchd_path(home_dir: &Path, binary_parent: Option<&Path>) -> String {
    let mut parts = Vec::new();

    if let Some(parent) = binary_parent {
        push_unique_path_entry(&mut parts, parent.display().to_string());
    }

    for entry in [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
        home_dir.join(".cargo/bin").display().to_string(),
        home_dir.join(".bun/bin").display().to_string(),
        home_dir.join(".local/bin").display().to_string(),
    ] {
        push_unique_path_entry(&mut parts, entry);
    }

    parts.join(":")
}

#[cfg(target_os = "macos")]
fn push_unique_path_entry(parts: &mut Vec<String>, entry: String) {
    if !parts.iter().any(|existing| existing == &entry) {
        parts.push(entry);
    }
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn render_launchd_plist(
    label: &str,
    binary_path: &Path,
    working_dir: &Path,
    moon_home: &Path,
    moon_logs_dir: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    home_dir: &Path,
    path_value: &str,
    moon_config_path: Option<&Path>,
) -> String {
    let config_entry = moon_config_path.map_or_else(String::new, |path| {
        format!(
            "    <key>MOON_CONFIG_PATH</key><string>{}</string>\n",
            xml_escape(&path.display().to_string())
        )
    });

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>-i</string>
    <string>-s</string>
    <string>{}</string>
    <string>watch</string>
    <string>--daemon</string>
  </array>
  <key>WorkingDirectory</key><string>{}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key><string>{}</string>
    <key>PATH</key><string>{}</string>
    <key>MOON_HOME</key><string>{}</string>
    <key>MOON_LOGS_DIR</key><string>{}</string>
{}
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml_escape(label),
        xml_escape(CAFFEINATE_PATH),
        xml_escape(&binary_path.display().to_string()),
        xml_escape(&working_dir.display().to_string()),
        xml_escape(&home_dir.display().to_string()),
        xml_escape(path_value),
        xml_escape(&moon_home.display().to_string()),
        xml_escape(&moon_logs_dir.display().to_string()),
        config_entry,
        xml_escape(&stdout_path.display().to_string()),
        xml_escape(&stderr_path.display().to_string()),
    )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::render_launchd_plist;
    use std::path::Path;

    #[test]
    fn render_launchd_plist_wraps_watcher_with_caffeinate() {
        let payload = render_launchd_plist(
            "com.moon.watch",
            Path::new("/Users/alice/.cargo/bin/moon"),
            Path::new("/Users/alice"),
            Path::new("/Users/alice/.moon"),
            Path::new("/Users/alice/.moon/logs"),
            Path::new("/Users/alice/.moon/logs/stdout.log"),
            Path::new("/Users/alice/.moon/logs/stderr.log"),
            Path::new("/Users/alice"),
            "/Users/alice/.cargo/bin:/usr/bin:/bin",
            None,
        );

        assert!(payload.contains("<string>/usr/bin/caffeinate</string>"));
        assert!(payload.contains("<string>-i</string>"));
        assert!(payload.contains("<string>-s</string>"));
        assert!(payload.contains("<string>/Users/alice/.cargo/bin/moon</string>"));
        assert!(payload.contains("<string>watch</string>"));
        assert!(payload.contains("<string>--daemon</string>"));
    }
}
