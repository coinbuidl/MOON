use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_moon_env_char(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
}

fn collect_moon_env_keys(source: &str, out: &mut BTreeSet<String>) {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i + 5 <= bytes.len() {
        if &bytes[i..i + 5] == b"MOON_" {
            let mut j = i + 5;
            while j < bytes.len() && is_moon_env_char(bytes[j]) {
                j += 1;
            }
            if j > i + 5 {
                if let Some(raw) = source.get(i..j) {
                    out.insert(raw.to_string());
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

fn write_generated_allowlist() -> std::io::Result<()> {
    let mut rs_files = Vec::new();
    collect_rs_files(Path::new("src"), &mut rs_files)?;

    let mut keys = BTreeSet::new();
    for file in rs_files {
        if let Ok(content) = fs::read_to_string(&file) {
            collect_moon_env_keys(&content, &mut keys);
        }
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let generated = Path::new(&out_dir).join("moon_env_allowlist.rs");
    let mut f = fs::File::create(generated)?;
    writeln!(f, "pub const GENERATED_MOON_ENV_ALLOWLIST: &[&str] = &[")?;
    for key in keys {
        writeln!(f, "    \"{key}\",")?;
    }
    writeln!(f, "];")?;
    Ok(())
}

fn main() {
    write_generated_allowlist().expect("failed to generate MOON env allowlist");

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();

    // Generate a simple unique-ish string for development/local use without adding dependencies.
    // In production, you might use a real UUID crate in build-dependencies.
    let build_id = format!("{:x}-{:x}", now.as_secs(), now.subsec_nanos());

    println!("cargo:rustc-env=BUILD_UUID={}", build_id);
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
}
