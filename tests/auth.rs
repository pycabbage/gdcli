use gdcli::auth::SCOPES;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const SECRET: &str = r#"{"installed":{"client_id":"fake.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;
const STORED_TOKEN: &str = r#"[{"scopes":["https://www.googleapis.com/auth/calendar.events.readonly","https://www.googleapis.com/auth/meetings.space.readonly","https://www.googleapis.com/auth/drive.readonly"],"token":{"access_token":"old","refresh_token":null,"expires_at":[2099,1,0,0,0,0,0,0,0],"id_token":null}}]"#;

struct Login(Child);

impl Drop for Login {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn authorization_url(
    config_home: &std::path::Path,
    data_home: &std::path::Path,
) -> (Login, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gdcli"))
        .arg("login")
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_DATA_HOME", data_home)
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

fn prepared(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let config = dir.join("cfg");
    let data = dir.join("data");
    std::fs::create_dir_all(config.join("gdcli")).unwrap();
    std::fs::write(config.join("gdcli/client_secret.json"), SECRET).unwrap();
    (config, data)
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
    let (config, data) = prepared(dir.path());
    let (_login, url) = authorization_url(&config, &data);
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
fn login_discards_the_stored_token_before_starting_the_flow() {
    let dir = tempfile::tempdir().unwrap();
    let (config, data) = prepared(dir.path());
    std::fs::create_dir_all(data.join("gdcli")).unwrap();
    let token = data.join("gdcli/token.json");
    std::fs::write(&token, STORED_TOKEN).unwrap();
    let (_login, url) = authorization_url(&config, &data);
    assert!(url.contains("access_type=offline"), "{url}");
    assert!(!token.exists());
}
