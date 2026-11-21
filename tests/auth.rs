use gdcli::auth::SCOPES;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

const SECRET: &str = r#"{"installed":{"client_id":"fake.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;
const PREFIX: &str = "Please direct your browser to ";
const SUFFIX: &str = " and follow the instructions displayed there.";

struct Login {
    child: Child,
    lines: Receiver<String>,
    seen: Vec<String>,
}

impl Drop for Login {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Login {
    fn url(&self) -> &str {
        let line = self.seen.last().unwrap();
        line.split_once(PREFIX)
            .unwrap()
            .1
            .strip_suffix(SUFFIX)
            .expect(line)
    }

    fn output(mut self) -> Vec<String> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(20)) {
            self.seen.push(line);
        }
        std::mem::take(&mut self.seen)
    }
}

fn no_launchers(dir: &Path) -> PathBuf {
    let path = dir.join("no-launchers");
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn start(dir: &Path, args: &[&str], path: &Path, env: &[(&str, &str)], input: &str) -> Login {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gdcli"))
        .args(args)
        .env_clear()
        .env("HOME", dir.join("home"))
        .env("XDG_CONFIG_HOME", dir.join("cfg"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("PATH", path)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut login = Login {
        child,
        lines,
        seen: Vec::new(),
    };
    loop {
        match login.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let found = line.contains("https://accounts.google.com/");
                login.seen.push(line);
                if found {
                    return login;
                }
            }
            Err(RecvTimeoutError::Timeout) => panic!("login printed no authorization URL"),
            Err(RecvTimeoutError::Disconnected) => panic!("login exited without a URL"),
        }
    }
}

fn authorization_url(dir: &Path) -> (Login, String) {
    let login = start(
        dir,
        &["auth", "login", "--no-browser"],
        &no_launchers(dir),
        &[],
        "",
    );
    let url = login.url().to_owned();
    (login, url)
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

#[cfg(target_os = "linux")]
mod launchers {
    use super::{Login, PREFIX, Path, PathBuf, prepared, start};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    fn stubs(dir: &Path) -> (PathBuf, PathBuf) {
        let bin = dir.join("stubs");
        std::fs::create_dir_all(&bin).unwrap();
        let log = dir.join("opened.log");
        let script = bin.join("xdg-open");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (bin, log)
    }

    fn wait_for(log: &Path) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !std::fs::read_to_string(log).is_ok_and(|text| text.ends_with('\n')) {
            assert!(Instant::now() < deadline, "the launcher never ran");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn opened_exactly_once(login: Login, log: &Path) {
        let url = login.url().to_owned();
        wait_for(log);
        let lines = login.output();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(std::fs::read_to_string(log).unwrap(), format!("{url}\n"));
    }

    fn never_opened(log: &Path) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            assert!(!log.exists(), "{:?}", std::fs::read_to_string(log));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        prepared(dir.path());
        let (bin, log) = stubs(dir.path());
        (dir, bin, log)
    }

    const AGENT: (&str, &str) = ("CLAUDE_CODE_CHILD_SESSION", "1");

    #[test]
    fn no_browser_prints_the_url_once_and_opens_nothing() {
        let (dir, bin, log) = fixture();
        let login = start(
            dir.path(),
            &["auth", "login", "--no-browser"],
            &bin,
            &[],
            "",
        );
        never_opened(&log);
        let lines = login.output();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(PREFIX), "{lines:?}");
        assert!(!log.exists());
    }

    #[test]
    fn an_agent_variable_prints_one_hint_then_the_url_and_opens_nothing() {
        let (dir, bin, log) = fixture();
        let login = start(dir.path(), &["auth", "login"], &bin, &[AGENT], "");
        never_opened(&log);
        let lines = login.output();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("GDCLI_OPEN_BROWSER=1"), "{lines:?}");
        assert!(lines[0].contains("--no-browser"), "{lines:?}");
        assert!(lines[0].contains("never on your own"), "{lines:?}");
        assert!(lines[1].starts_with(PREFIX), "{lines:?}");
        assert!(!log.exists());
    }

    #[test]
    fn no_agent_variable_opens_the_browser_once_with_the_printed_url() {
        let (dir, bin, log) = fixture();
        let login = start(dir.path(), &["auth", "login"], &bin, &[], "");
        opened_exactly_once(login, &log);
    }

    #[test]
    fn gdcli_open_browser_1_opens_the_browser_inside_an_agent_shell() {
        let (dir, bin, log) = fixture();
        let env = [AGENT, ("GDCLI_OPEN_BROWSER", "1")];
        let login = start(dir.path(), &["auth", "login"], &bin, &env, "");
        opened_exactly_once(login, &log);
    }

    #[test]
    fn setup_with_no_browser_prints_the_url_once_and_opens_nothing() {
        let (dir, bin, log) = fixture();
        let login = start(
            dir.path(),
            &["auth", "setup", "--no-browser"],
            &bin,
            &[],
            "\n",
        );
        never_opened(&log);
        let lines = login.output();
        let printed: usize = lines.iter().map(|line| line.matches(PREFIX).count()).sum();
        assert_eq!(printed, 1, "{lines:?}");
        assert!(!log.exists());
    }

    #[test]
    fn setup_opens_the_browser_once_with_the_printed_url() {
        let (dir, bin, log) = fixture();
        let login = start(dir.path(), &["auth", "setup"], &bin, &[], "\n");
        let url = login.url().to_owned();
        wait_for(&log);
        let lines = login.output();
        let printed: usize = lines.iter().map(|line| line.matches(PREFIX).count()).sum();
        assert_eq!(printed, 1, "{lines:?}");
        assert_eq!(std::fs::read_to_string(&log).unwrap(), format!("{url}\n"));
    }

    #[test]
    fn setup_in_an_agent_shell_prints_the_hint_before_the_url_and_opens_nothing() {
        let (dir, bin, log) = fixture();
        let login = start(dir.path(), &["auth", "setup"], &bin, &[AGENT], "\n");
        never_opened(&log);
        let text = login.output().join("\n");
        let hint = "No browser was opened";
        assert_eq!(text.matches(hint).count(), 1, "{text}");
        assert_eq!(text.matches(PREFIX).count(), 1, "{text}");
        assert!(text.find(hint) < text.find(PREFIX), "{text}");
        assert!(!log.exists());
    }
}
