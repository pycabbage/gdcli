use std::path::Path;
use std::process::{Command, Output};

const SECRET: &str = r#"{"installed":{"client_id":"id.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;

fn gdcli(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gdcli"));
    command
        .args(args)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME");
    command
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn place_secret(config_home: &Path) {
    let dir = config_home.join("gdcli");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("client_secret.json"), SECRET).unwrap();
}

#[test]
fn missing_client_secret_points_at_setup_and_the_xdg_path() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["calendar", "list"][..],
        &["meet", "get", "abc-defg-hij"],
        &["docs", "get", "X"],
    ] {
        let output = gdcli(args)
            .env("XDG_CONFIG_HOME", dir.path().join("cfg"))
            .env("XDG_DATA_HOME", dir.path().join("data"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        let message = stderr(&output);
        assert!(message.contains("gdcli setup"), "{message}");
        assert!(
            message.contains(&format!(
                "{}/cfg/gdcli/client_secret.json",
                dir.path().display()
            )),
            "{message}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn xdg_defaults_fall_back_to_home() {
    let dir = tempfile::tempdir().unwrap();
    let output = gdcli(&["calendar", "list"])
        .env("HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        stderr(&output).contains(&format!(
            "{}/.config/gdcli/client_secret.json",
            dir.path().display()
        )),
        "{}",
        stderr(&output)
    );
}

#[test]
fn missing_token_fails_fast_pointing_at_login_without_starting_a_flow() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    for args in [
        &["calendar", "list"][..],
        &["calendar", "get", "e"],
        &["meet", "list"],
        &["meet", "get", "abc-defg-hij"],
        &["docs", "get", "X"],
    ] {
        let output = gdcli(args)
            .env("XDG_CONFIG_HOME", dir.path().join("cfg"))
            .env("XDG_DATA_HOME", dir.path().join("data"))
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        let message = stderr(&output);
        assert!(message.contains("gdcli login"), "{message}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    assert!(!dir.path().join("data").exists());
}

#[test]
fn unknown_flags_exit_with_usage_error() {
    let output = gdcli(&["docs", "get", "X", "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--json"));
}

#[test]
fn help_and_version_work_without_any_configuration() {
    let dir = tempfile::tempdir().unwrap();
    for args in [&["--help"][..], &["--version"], &["meet", "--help"]] {
        let output = gdcli(args).env("HOME", dir.path()).output().unwrap();
        assert!(output.status.success(), "{args:?}");
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn corrupt_token_file_points_at_login() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    std::fs::create_dir_all(dir.path().join("data/gdcli")).unwrap();
    std::fs::write(dir.path().join("data/gdcli/token.json"), "garbage").unwrap();
    let output = gdcli(&["meet", "list"])
        .env("XDG_CONFIG_HOME", dir.path().join("cfg"))
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("token.json"), "{message}");
    assert!(message.contains("gdcli login"), "{message}");
}
