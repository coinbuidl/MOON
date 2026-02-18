use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path) {
    let script = "#!/usr/bin/env bash\necho '[{\"path\":\"/tmp/a.json\",\"snippet\":\"rule captured\",\"score\":0.8}]'\n";
    fs::write(bin_path, script).expect("write fake qmd");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(bin_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_path, perms).expect("chmod");
    }
}

#[test]
fn moon_recall_returns_matches() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("moon-recall")
        .args(["--query", "rule"])
        .assert()
        .success();
}
