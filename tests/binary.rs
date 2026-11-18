use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

const SECRET: &str = r#"{"installed":{"client_id":"id.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;

fn gdcli(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gdcli"));
    command
        .args(args)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME");
    command
}

fn isolated(args: &[&str], dir: &Path) -> Command {
    let mut command = gdcli(args);
    command
        .env("XDG_CONFIG_HOME", dir.join("cfg"))
        .env("XDG_DATA_HOME", dir.join("data"));
    command
}

fn wait(mut child: Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("gdcli did not finish: it must not wait for a login");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run(command: &mut Command) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("stdout");
    let err = dir.path().join("stderr");
    let child = command
        .stdin(Stdio::null())
        .stdout(File::create(&out).unwrap())
        .stderr(File::create(&err).unwrap())
        .spawn()
        .unwrap();
    Output {
        status: wait(child),
        stdout: std::fs::read(out).unwrap(),
        stderr: std::fs::read(err).unwrap(),
    }
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
        let output = run(&mut isolated(args, dir.path()));
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
        assert_eq!(output.stdout, b"");
    }
}

#[test]
fn xdg_defaults_fall_back_to_home() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(gdcli(&["calendar", "list"]).env("HOME", dir.path()));
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
        let output = run(&mut isolated(args, dir.path()));
        assert!(!output.status.success(), "{args:?}");
        let message = stderr(&output);
        assert!(message.contains("gdcli login"), "{message}");
        assert_eq!(output.stdout, b"", "{args:?}");
    }
    assert!(!dir.path().join("data").exists());
}

#[test]
fn unknown_flags_exit_with_usage_error() {
    let output = run(&mut gdcli(&["docs", "get", "X", "--json"]));
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--json"));
}

#[test]
fn help_and_version_work_without_any_configuration() {
    let dir = tempfile::tempdir().unwrap();
    for args in [&["--help"][..], &["--version"], &["meet", "--help"]] {
        let output = run(gdcli(args).env("HOME", dir.path()));
        assert!(output.status.success(), "{args:?}");
        assert_ne!(output.stdout, b"");
    }
}

#[test]
fn corrupt_token_file_points_at_login() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    std::fs::create_dir_all(dir.path().join("data/gdcli")).unwrap();
    std::fs::write(dir.path().join("data/gdcli/token.json"), "garbage").unwrap();
    let output = run(&mut isolated(&["meet", "list"], dir.path()));
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("token.json"), "{message}");
    assert!(message.contains("gdcli login"), "{message}");
}

#[test]
fn error_output_starts_with_a_lowercase_letter() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    for args in [
        &["calendar", "list"][..],
        &["docs", "get", "X"],
        &["meet", "list"],
    ] {
        let output = run(&mut isolated(args, dir.path()));
        let message = stderr(&output);
        let text = message.strip_prefix("error: ").expect(&message);
        assert!(text.starts_with(char::is_lowercase), "{message}");
        assert!(text.contains("token retrieval failed"), "{message}");
    }
}

#[test]
fn a_closed_stdout_exits_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let err = dir.path().join("stderr");
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let child = gdcli(&["setup"])
        .env("XDG_CONFIG_HOME", dir.path().join("cfg"))
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(File::create(&err).unwrap())
        .spawn()
        .unwrap();
    assert!(wait(child).success());
    assert_eq!(std::fs::read_to_string(err).unwrap(), "");
}

#[test]
fn empty_ids_exit_with_a_usage_error() {
    for args in [
        &["calendar", "get", ""][..],
        &["meet", "get", ""],
        &["docs", "get", ""],
    ] {
        let output = run(&mut gdcli(args));
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(output.stdout, b"");
    }
}

#[test]
fn relative_xdg_values_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(gdcli(&["calendar", "list"])
        .env("XDG_CONFIG_HOME", "relative/config")
        .env("XDG_DATA_HOME", "")
        .env("HOME", dir.path()));
    let expected = format!("{}/.config/gdcli/client_secret.json", dir.path().display());
    assert!(stderr(&output).contains(&expected), "{}", stderr(&output));
}

#[test]
fn a_blank_tab_exits_with_a_usage_error_before_any_request() {
    for tab in ["", "  "] {
        let output = run(&mut gdcli(&["docs", "get", "DOC", "--tab", tab]));
        assert_eq!(output.status.code(), Some(2), "{tab:?}");
        assert_eq!(output.stdout, b"");
        assert!(
            stderr(&output).contains("invalid tab"),
            "{}",
            stderr(&output)
        );
    }
}

#[test]
fn the_reported_version_is_the_package_version() {
    let output = run(&mut gdcli(&["--version"]));
    let expected = format!("gdcli {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}
