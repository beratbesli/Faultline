use std::fs;
use std::process::Command;

#[test]
fn doctor_does_not_print_database_credentials() {
    let workspace = tempfile::tempdir().expect("temporary workspace");
    let config = workspace.path().join("faultline.yaml");
    fs::write(
        &config,
        "database:\n  url_env: null\n  url: 'postgres://user:supersecret@127.0.0.1:1/test?token=private_token'\nmigration:\n  up_sql: migration.sql\n",
    )
    .expect("write config");
    let output = Command::new(env!("CARGO_BIN_EXE_faultline"))
        .current_dir(workspace.path())
        .args(["--config", config.to_str().expect("config path"), "doctor"])
        .output()
        .expect("run doctor");
    assert!(output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains("supersecret"), "{combined}");
    assert!(!combined.contains("private_token"), "{combined}");
}
