use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;

use crate::commands;

#[derive(Debug, Parser)]
#[command(name = "moon")]
#[command(about = "OpenClaw context optimization installer/repair CLI")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true)]
    pub allow_out_of_bounds: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Install(InstallArgs),
    Verify(VerifyArgs),
    Repair(RepairArgs),
    PostUpgrade,
    Status,
    MoonStatus,
    MoonStop,
    MoonSnapshot(MoonSnapshotArgs),
    MoonIndex(MoonIndexArgs),
    MoonWatch(MoonWatchArgs),
    MoonEmbed(MoonEmbedArgs),
    MoonRecall(MoonRecallArgs),
    #[command(name = "distill", visible_alias = "moon-distill")]
    Distill(DistillArgs),
    Config(ConfigArgs),
    MoonHealth,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub apply: bool,
}

#[derive(Debug, Args, Default)]
pub struct VerifyArgs {
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Args, Default)]
pub struct RepairArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Default)]
pub struct MoonSnapshotArgs {
    #[arg(long)]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct MoonIndexArgs {
    #[arg(long, default_value = "history")]
    pub name: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub reproject: bool,
}

#[derive(Debug, Args, Default)]
pub struct MoonWatchArgs {
    #[arg(long)]
    pub once: bool,
    #[arg(long)]
    pub daemon: bool,
    #[arg(long)]
    pub distill_now: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct MoonRecallArgs {
    #[arg(long)]
    pub query: String,
    #[arg(long, default_value = "history")]
    pub name: String,
    #[arg(long)]
    pub channel_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct MoonEmbedArgs {
    #[arg(long, default_value = "history")]
    pub name: String,
    #[arg(long, default_value_t = 25)]
    pub max_docs: usize,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub watcher_trigger: bool,
}

#[derive(Debug, Args)]
pub struct DistillArgs {
    #[arg(long = "mode", default_value = "norm")]
    pub mode: String,
    #[arg(long = "archive")]
    pub archive: Option<String>,
    #[arg(long = "file")]
    pub files: Vec<String>,
    #[arg(long = "session-id")]
    pub session_id: Option<String>,
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Debug, Args, Default)]
pub struct ConfigArgs {
    #[arg(long)]
    pub show: bool,
}

fn print_report(report: &commands::CommandReport, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("command: {}", report.command);
    println!("ok: {}", report.ok);
    if !report.details.is_empty() {
        println!("details:");
        for detail in &report.details {
            println!("- {detail}");
        }
    }
    if !report.issues.is_empty() {
        println!("issues:");
        for issue in &report.issues {
            println!("- {issue}");
        }
    }
    Ok(())
}

fn normalize_single_dash_long_flags() -> Vec<OsString> {
    std::env::args_os()
        .map(|arg| {
            let Some(raw) = arg.to_str() else {
                return arg;
            };

            let rewritten = match raw {
                "-mode" => Some("--mode".to_string()),
                "-archive" => Some("--archive".to_string()),
                "-file" => Some("--file".to_string()),
                "-session-id" => Some("--session-id".to_string()),
                "-dry-run" => Some("--dry-run".to_string()),
                _ if raw.starts_with("-mode=")
                    || raw.starts_with("-archive=")
                    || raw.starts_with("-file=")
                    || raw.starts_with("-session-id=")
                    || raw.starts_with("-dry-run=") =>
                {
                    Some(format!("--{}", &raw[1..]))
                }
                _ => None,
            };

            rewritten.map(OsString::from).unwrap_or(arg)
        })
        .collect()
}

pub fn run() -> Result<()> {
    let cli = Cli::parse_from(normalize_single_dash_long_flags());
    let paths = crate::moon::paths::resolve_paths()?;

    // Every command validates CWD except diagnostics.
    match &cli.command {
        Command::Status
        | Command::MoonStatus
        | Command::MoonHealth
        | Command::Verify(_)
        | Command::Config(_) => {
            // Diagnostics are exempt from CWD enforcement.
        }
        _ => {
            commands::validate_cwd(&paths, cli.allow_out_of_bounds)?;
        }
    }

    let report = match &cli.command {
        Command::Install(args) => commands::install::run(&commands::install::InstallOptions {
            force: args.force,
            dry_run: args.dry_run,
            apply: args.apply,
        })?,
        Command::Verify(args) => commands::verify::run(&commands::verify::VerifyOptions {
            strict: args.strict,
        })?,
        Command::Repair(args) => {
            commands::repair::run(&commands::repair::RepairOptions { force: args.force })?
        }
        Command::PostUpgrade => commands::post_upgrade::run()?,
        Command::Status => commands::status::run()?,
        Command::MoonStatus => commands::moon_status::run()?,
        Command::MoonStop => commands::moon_stop::run()?,
        Command::MoonSnapshot(args) => {
            commands::moon_snapshot::run(&commands::moon_snapshot::MoonSnapshotOptions {
                source: args.source.clone(),
                dry_run: args.dry_run,
            })?
        }
        Command::MoonIndex(args) => {
            commands::moon_index::run(&commands::moon_index::MoonIndexOptions {
                collection_name: args.name.clone(),
                dry_run: args.dry_run,
                reproject: args.reproject,
            })?
        }
        Command::MoonWatch(args) => {
            commands::moon_watch::run(&commands::moon_watch::MoonWatchOptions {
                once: args.once,
                daemon: args.daemon,
                distill_now: args.distill_now,
                dry_run: args.dry_run,
            })?
        }
        Command::MoonEmbed(args) => {
            commands::moon_embed::run(&commands::moon_embed::MoonEmbedOptions {
                collection_name: args.name.clone(),
                max_docs: args.max_docs,
                dry_run: args.dry_run,
                watcher_trigger: args.watcher_trigger,
            })?
        }
        Command::MoonRecall(args) => {
            commands::moon_recall::run(&commands::moon_recall::MoonRecallOptions {
                query: args.query.clone(),
                collection_name: args.name.clone(),
                channel_key: args.channel_key.clone(),
            })?
        }
        Command::Distill(args) => {
            commands::moon_distill::run(&commands::moon_distill::MoonDistillOptions {
                mode: args.mode.clone(),
                archive_path: args.archive.clone(),
                files: args.files.clone(),
                session_id: args.session_id.clone(),
                dry_run: args.dry_run,
            })?
        }
        Command::Config(args) => {
            commands::moon_config::run(&commands::moon_config::MoonConfigOptions {
                show: args.show,
            })?
        }
        Command::MoonHealth => commands::moon_health::run()?,
    };

    print_report(&report, cli.json)?;

    if report.ok {
        Ok(())
    } else {
        std::process::exit(2);
    }
}
