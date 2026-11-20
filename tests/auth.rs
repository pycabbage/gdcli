use gdcli::auth::SCOPES;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const SECRET: &str = r#"{"installed":{"client_id":"fake.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;

struct Login(Child);

impl Drop for Login {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn authorization_url(dir: &std::path::Path) -> (Login, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gdcli"))
        .args(["auth", "login"])
        .env_clear()
        .env("HOME", dir.join("home"))
        .env("XDG_CONFIG_HOME", dir.join("cfg"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let guard = Login(child);
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(line) if line.contains("https://accounts.google.com/") => return (guard, line),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => panic!("login printed no authorization URL"),
            Err(RecvTimeoutError::Disconnected) => panic!("login exited without a URL"),
        }
    }
}

fn prepared(dir: &std::path::Path) {
    let config = dir.join("cfg/gdcli");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("client_secret.json"), SECRET).unwrap();
}

#[test]
fn scopes_match_the_ones_documented_in_setup() {
    let setup = include_str!("../docs/setup.md");
    let mut documented: Vec<&str> = setup
        .split_whitespace()
        .filter(|word| word.starts_with("https://www.googleapis.com/auth/"))
        .collect();
    documented.sort_unstable();
    let mut actual = SCOPES.to_vec();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            "https://www.googleapis.com/auth/calendar.events.readonly",
            "https://www.googleapis.com/auth/drive.readonly",
            "https://www.googleapis.com/auth/meetings.space.readonly",
        ]
    );
    assert_eq!(documented, actual);
}

#[test]
fn login_url_requests_offline_access_with_consent_and_all_scopes() {
    let dir = tempfile::tempdir().unwrap();
    prepared(dir.path());
    std::fs::create_dir_all(dir.path().join("data/gdcli")).unwrap();
    std::fs::write(dir.path().join("data/gdcli/token.json"), "garbage").unwrap();
    let (_login, url) = authorization_url(dir.path());
    assert!(url.contains("access_type=offline"), "{url}");
    assert!(url.contains("prompt=select_account+consent"), "{url}");
    assert!(url.contains("redirect_uri=http://localhost:"), "{url}");
    assert!(
        url.contains("client_id=fake.apps.googleusercontent.com"),
        "{url}"
    );
    for scope in SCOPES {
        assert!(url.contains(scope), "{url}");
    }
}

#[test]
fn login_keeps_every_account_until_the_login_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    prepared(dir.path());
    let accounts = dir.path().join("data/gdcli/accounts");
    std::fs::create_dir_all(&accounts).unwrap();
    let account = accounts.join("alice@example.com.json");
    std::fs::write(&account, "not a token").unwrap();
    let (_login, url) = authorization_url(dir.path());
    assert!(url.contains("access_type=offline"), "{url}");
    assert_eq!(std::fs::read_to_string(&account).unwrap(), "not a token");
    let mut names: Vec<String> = std::fs::read_dir(&accounts)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[1], "alice@example.com.json");
    assert!(names[0].starts_with(".login-"), "{names:?}");
    assert!(!dir.path().join("data/gdcli/default").exists());
}
