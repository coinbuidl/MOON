use std::time::SystemTime;

fn main() {
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
