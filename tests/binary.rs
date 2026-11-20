use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

const SECRET: &str = r#"{"installed":{"client_id":"id.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;

const EXPIRED_TOKEN: &str = r#"[{"scopes":["https://www.googleapis.com/auth/calendar.events.readonly","https://www.googleapis.com/auth/meetings.space.readonly","https://www.googleapis.com/auth/drive.readonly"],"token":{"access_token":"old","refresh_token":null,"expires_at":[2000,1,0,0,0,0,0,0,0],"id_token":null}}]"#;

fn gdcli(args: &[&str], dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gdcli"));
    command.args(args).env_clear().env("HOME", dir.join("home"));
    command
}

fn isolated(args: &[&str], dir: &Path) -> Command {
    let mut command = gdcli(args, dir);
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

fn place_account(dir: &Path, email: &str, content: &str) {
    let accounts = dir.join("data/gdcli/accounts");
    std::fs::create_dir_all(&accounts).unwrap();
    std::fs::write(accounts.join(format!("{email}.json")), content).unwrap();
}

fn place_default(dir: &Path, email: &str) {
    std::fs::create_dir_all(dir.join("data/gdcli")).unwrap();
    std::fs::write(dir.join("data/gdcli/default"), format!("{email}\n")).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn missing_client_secret_points_at_setup_and_the_xdg_path() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["calendar", "list"][..],
        &["meet", "get", "abc-defg-hij"],
        &["docs", "get", "X"],
        &["auth", "login"],
    ] {
        let output = run(&mut isolated(args, dir.path()));
        assert!(!output.status.success());
        let message = stderr(&output);
        assert!(message.contains("gdcli auth setup"), "{message}");
        assert!(
            message.contains(&format!(
                "{}/cfg/gdcli/client_secret.json",
                dir.path().display()
            )),
            "{message}"
        );
        assert_eq!(output.stdout, b"");
        assert!(!dir.path().join("data").exists(), "{args:?}");
    }
}

#[test]
fn xdg_defaults_fall_back_to_home() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(gdcli(&["calendar", "list"], dir.path()).env("HOME", dir.path()));
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
        assert_eq!(
            message.trim_end(),
            "error: no account is logged in: run `gdcli auth login`",
            "{args:?}"
        );
        assert_eq!(output.stdout, b"", "{args:?}");
    }
    assert!(!dir.path().join("data").exists());
}

#[test]
fn unknown_flags_exit_with_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&mut gdcli(&["docs", "get", "X", "--json"], dir.path()));
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--json"));
}

#[test]
fn help_and_version_work_without_any_configuration() {
    let dir = tempfile::tempdir().unwrap();
    for args in [&["--help"][..], &["--version"], &["meet", "--help"]] {
        let output = run(gdcli(args, dir.path()).env("HOME", dir.path()));
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
    for args in [
        &["meet", "list"][..],
        &["auth", "list"],
        &["auth", "set-default", "a@b.c"],
    ] {
        let output = run(&mut isolated(args, dir.path()));
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(output.stdout, b"", "{args:?}");
        let message = stderr(&output);
        assert!(message.contains("data/gdcli/token.json"), "{message}");
        assert!(message.contains("gdcli auth login"), "{message}");
    }
}

#[test]
fn error_output_starts_with_a_lowercase_letter() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    place_account(dir.path(), "alice@example.com", EXPIRED_TOKEN);
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
    let child = gdcli(&["auth", "setup"], dir.path())
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
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["calendar", "get", ""][..],
        &["meet", "get", ""],
        &["docs", "get", ""],
    ] {
        let output = run(&mut gdcli(args, dir.path()));
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(output.stdout, b"");
    }
}

#[test]
fn relative_xdg_values_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(gdcli(&["calendar", "list"], dir.path())
        .env("XDG_CONFIG_HOME", "relative/config")
        .env("XDG_DATA_HOME", "")
        .env("HOME", dir.path()));
    let expected = format!("{}/.config/gdcli/client_secret.json", dir.path().display());
    assert!(stderr(&output).contains(&expected), "{}", stderr(&output));
}

#[test]
fn a_blank_tab_exits_with_a_usage_error_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    for tab in ["", "  "] {
        let output = run(&mut gdcli(
            &["docs", "get", "DOC", "--tab", tab],
            dir.path(),
        ));
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
    let dir = tempfile::tempdir().unwrap();
    let output = run(&mut gdcli(&["--version"], dir.path()));
    let expected = format!("gdcli {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn auth_list_prints_the_header_and_marks_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&mut isolated(&["auth", "list"], dir.path()));
    assert!(output.status.success());
    assert_eq!(stdout(&output), "account\tdefault\n");
    assert!(!dir.path().join("data").exists());
    place_account(dir.path(), "bob@example.org", "[]");
    place_account(dir.path(), "alice@example.com", "[]");
    place_default(dir.path(), "bob@example.org");
    let output = run(&mut isolated(&["auth", "list"], dir.path()));
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "account\tdefault\nalice@example.com\tfalse\nbob@example.org\ttrue\n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn auth_list_marks_none_when_the_default_is_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    place_account(dir.path(), "bob@example.org", "[]");
    place_account(dir.path(), "alice@example.com", "[]");
    let expected = "account\tdefault\nalice@example.com\tfalse\nbob@example.org\tfalse\n";
    let output = run(&mut isolated(&["auth", "list"], dir.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), expected);
    place_default(dir.path(), "ghost@example.com");
    let output = run(&mut isolated(&["auth", "list"], dir.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), expected);
}

#[test]
fn auth_set_default_updates_the_pointer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    place_account(dir.path(), "alice@example.com", "[]");
    place_account(dir.path(), "bob@example.org", "[]");
    place_default(dir.path(), "alice@example.com");
    let pointer = dir.path().join("data/gdcli/default");
    let output = run(&mut isolated(
        &["auth", "set-default", "Bob@Example.org"],
        dir.path(),
    ));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "account: bob@example.org\ndefault: true\n");
    assert_eq!(
        std::fs::read_to_string(&pointer).unwrap(),
        "bob@example.org\n"
    );
    let mode = std::fs::metadata(&pointer).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let output = run(&mut isolated(
        &["auth", "set-default", "carol@example.net"],
        dir.path(),
    ));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        stderr(&output).trim_end(),
        "error: unknown account carol@example.net; known accounts: alice@example.com, bob@example.org"
    );
    assert_eq!(
        std::fs::read_to_string(&pointer).unwrap(),
        "bob@example.org\n"
    );
}

#[test]
fn unknown_account_names_the_known_ones_before_any_network_call() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    place_account(dir.path(), "alice@example.com", "[]");
    let message = |args: &[&str]| {
        let output = run(&mut isolated(args, dir.path()));
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(output.stdout, b"", "{args:?}");
        stderr(&output).trim_end().to_owned()
    };
    assert_eq!(
        message(&["calendar", "list", "-a", "Bob@Example.com"]),
        "error: unknown account bob@example.com; known accounts: alice@example.com"
    );
    assert_eq!(
        message(&["docs", "get", "X", "--account", "bob@example.com"]),
        "error: unknown account bob@example.com; known accounts: alice@example.com"
    );
    place_default(dir.path(), "ghost@example.com");
    assert_eq!(
        message(&["meet", "list"]),
        "error: default account ghost@example.com has no token: run `gdcli auth login` or `gdcli auth set-default <email>`; known accounts: alice@example.com"
    );
    place_account(dir.path(), "bob@example.org", "[]");
    std::fs::remove_file(dir.path().join("data/gdcli/default")).unwrap();
    assert_eq!(
        message(&["calendar", "get", "e"]),
        "error: no default account; run `gdcli auth set-default <email>`; known accounts: alice@example.com, bob@example.org"
    );
}

#[test]
fn corrupt_account_file_names_the_file_and_auth_login() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    place_account(dir.path(), "alice@example.com", "garbage");
    let output = run(&mut isolated(&["calendar", "list"], dir.path()));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    let message = stderr(&output);
    assert!(
        message.contains(&format!(
            "{}/data/gdcli/accounts/alice@example.com.json",
            dir.path().display()
        )),
        "{message}"
    );
    assert!(message.contains("run `gdcli auth login`"), "{message}");
}

#[test]
fn an_expired_token_names_the_account() {
    let dir = tempfile::tempdir().unwrap();
    place_secret(&dir.path().join("cfg"));
    place_account(dir.path(), "alice@example.com", EXPIRED_TOKEN);
    place_account(dir.path(), "bob@example.org", EXPIRED_TOKEN);
    for (args, who) in [
        (
            &["calendar", "list", "-a", "bob@example.org"][..],
            "bob@example.org",
        ),
        (
            &["calendar", "list", "-a", "alice@example.com"],
            "alice@example.com",
        ),
    ] {
        let output = run(&mut isolated(args, dir.path()));
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(output.stdout, b"");
        let message = stderr(&output);
        assert!(
            message.contains(&format!(
                "no valid token for {who}: run `gdcli auth login` and choose that account"
            )),
            "{message}"
        );
    }
}
