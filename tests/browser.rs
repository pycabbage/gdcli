use gdcli::Paths;
use gdcli::auth::browser::{Launch, Terminal, launch};
use gdcli::auth::login;
use gdcli::auth::setup::setup;
use std::ffi::OsString;
use std::fmt::Debug;
use std::future::Future;
use std::io::{self, Cursor, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;

const SECRET: &str = r#"{"installed":{"client_id":"fake.apps.googleusercontent.com","client_secret":"s","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#;
const PREFIX: &str = "Please direct your browser to ";
const SUFFIX: &str = " and follow the instructions displayed there.";
const HINT: &str = "No browser was opened because an AI agent environment was detected. Set GDCLI_OPEN_BROWSER=1 only after the user has agreed to a browser opening on their machine, never on your own; otherwise pass --no-browser and hand the printed URL to the user.";
const NOTHING: [&str; 0] = [];
const URL: &str = "https://accounts.google.com/o/oauth2/auth?client_id=x";
const AGENTS: [&str; 11] = [
    "AI_AGENT",
    "OPENCODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CODEX_THREAD_ID",
    "CODEX_CI",
    "GEMINI_CLI",
    "CURSOR_AGENT",
    "GOOSE_TERMINAL",
    "CLINE_ACTIVE",
    "OPENCLAW_SHELL",
    "AUGMENT_AGENT",
];

#[derive(Clone, Default)]
struct Probe {
    printed: Arc<Mutex<Vec<u8>>>,
    opened: Arc<Mutex<Vec<String>>>,
    printed_first: Arc<Mutex<Vec<bool>>>,
}

impl Write for Probe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.printed.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Probe {
    fn text(&self) -> String {
        String::from_utf8(self.printed.lock().unwrap().clone()).unwrap()
    }

    fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }

    fn printed_first(&self) -> Vec<bool> {
        self.printed_first.lock().unwrap().clone()
    }

    fn terminal(
        &self,
        launch: Launch,
        fail: bool,
    ) -> Terminal<Probe, impl Fn(&str) -> io::Result<()> + Send + Sync + 'static> {
        let probe = self.clone();
        Terminal::new(launch, self.clone(), move |url: &str| {
            probe.opened.lock().unwrap().push(url.to_owned());
            probe
                .printed_first
                .lock()
                .unwrap()
                .push(!probe.printed.lock().unwrap().is_empty());
            if fail {
                Err(io::Error::other("no launcher"))
            } else {
                Ok(())
            }
        })
    }
}

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    }
}

fn launched(no_browser: bool, pairs: &[(&str, &str)]) -> Launch {
    launch(no_browser, &lookup(pairs))
}

fn url_line(url: &str) -> String {
    format!("{PREFIX}{url}{SUFFIX}\n")
}

fn printed_url(text: &str) -> &str {
    assert_eq!(text.matches(PREFIX).count(), 1, "{text}");
    let url = text
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX)?.strip_suffix(SUFFIX))
        .expect(text);
    assert!(url.contains("client_id=fake.apps.googleusercontent.com"));
    url
}

fn hint_then_url(text: &str) -> &str {
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[0], HINT, "{text}");
    assert!(lines[1].starts_with(PREFIX), "{text}");
    printed_url(text)
}

async fn presented(launch: Launch, url: &str, fail: bool) -> Probe {
    let probe = Probe::default();
    let code = probe
        .terminal(launch, fail)
        .present_user_url(url, false)
        .await
        .unwrap();
    assert_eq!(code, "");
    let code = Probe::default()
        .terminal(launch, fail)
        .present_user_url(url, true)
        .await
        .unwrap();
    assert_eq!(code, "");
    probe
}

fn prepared(dir: &Path) -> Paths {
    let paths = Paths::new(dir.join("cfg"), dir.join("data"));
    std::fs::create_dir_all(&paths.config).unwrap();
    std::fs::write(paths.client_secret(), SECRET).unwrap();
    paths
}

async fn until_printed(probe: &Probe) {
    while !probe.text().contains("https://accounts.google.com/") {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn until_the_url<T: Debug>(probe: &Probe, running: impl Future<Output = anyhow::Result<T>>) {
    tokio::select! {
        result = running => panic!("ended before the URL: {result:?}"),
        waited = tokio::time::timeout(Duration::from_secs(20), until_printed(probe)) => {
            waited.expect("no URL");
        }
    }
}

async fn login_until_the_url(launch: Launch, fail: bool) -> Probe {
    let dir = tempfile::tempdir().unwrap();
    let paths = prepared(dir.path());
    let probe = Probe::default();
    let mut out = Vec::new();
    until_the_url(
        &probe,
        login(&paths, &mut out, probe.terminal(launch, fail)),
    )
    .await;
    assert_eq!(out, b"");
    probe
}

#[test]
fn launch_follows_the_precedence_table() {
    let agent = ("CLAUDE_CODE_CHILD_SESSION", "1");
    let opt_in = ("GDCLI_OPEN_BROWSER", "1");
    for (no_browser, pairs, expected) in [
        (true, &[][..], Launch::Skip),
        (true, &[agent][..], Launch::Skip),
        (true, &[opt_in][..], Launch::Skip),
        (true, &[agent, opt_in][..], Launch::Skip),
        (false, &[opt_in][..], Launch::Open),
        (false, &[agent, opt_in][..], Launch::Open),
        (false, &[agent][..], Launch::SkipWithHint),
        (false, &[][..], Launch::Open),
    ] {
        assert_eq!(
            launched(no_browser, pairs),
            expected,
            "{no_browser} {pairs:?}"
        );
    }
}

#[test]
fn every_listed_agent_variable_is_detected_alone() {
    for name in AGENTS {
        assert_eq!(
            launched(false, &[(name, "1")]),
            Launch::SkipWithHint,
            "{name}"
        );
        assert_eq!(
            launched(false, &[(name, "x")]),
            Launch::SkipWithHint,
            "{name}"
        );
        assert_eq!(launched(true, &[(name, "1")]), Launch::Skip, "{name}");
        assert_eq!(
            launched(false, &[(name, "1"), ("GDCLI_OPEN_BROWSER", "1")]),
            Launch::Open,
            "{name}"
        );
    }
}

#[test]
fn empty_values_and_other_names_are_not_agents() {
    for name in AGENTS {
        assert_eq!(launched(false, &[(name, "")]), Launch::Open, "{name}");
    }
    for name in [
        "CLAUDECODE",
        "CLAUDE_CODE",
        "OPENCODE_TERMINAL",
        "AGENT",
        "CURSOR_TRACE_ID",
    ] {
        assert_eq!(launched(false, &[(name, "1")]), Launch::Open, "{name}");
    }
}

#[test]
fn only_the_value_1_opts_in() {
    for value in ["0", "", "true", "yes", "1 ", "2"] {
        let opt_in = ("GDCLI_OPEN_BROWSER", value);
        assert_eq!(
            launched(false, &[("AI_AGENT", "1"), opt_in]),
            Launch::SkipWithHint,
            "{value:?}"
        );
        assert_eq!(launched(false, &[opt_in]), Launch::Open, "{value:?}");
    }
}

#[tokio::test]
async fn terminal_opens_and_prints_the_url_once() {
    let probe = presented(Launch::Open, URL, false).await;
    assert_eq!(probe.text(), url_line(URL));
    assert_eq!(probe.opened(), [URL]);
    assert_eq!(probe.printed_first(), [true]);
}

#[tokio::test]
async fn terminal_prints_the_url_once_when_the_opener_fails() {
    let probe = presented(Launch::Open, URL, true).await;
    assert_eq!(probe.text(), url_line(URL));
    assert_eq!(probe.opened(), [URL]);
    assert_eq!(probe.printed_first(), [true]);
}

#[tokio::test]
async fn terminal_skip_prints_the_url_once_and_never_opens() {
    let probe = presented(Launch::Skip, URL, false).await;
    assert_eq!(probe.text(), url_line(URL));
    assert_eq!(probe.opened(), NOTHING);
}

#[tokio::test]
async fn terminal_skip_with_hint_prints_the_hint_then_the_url_once_and_never_opens() {
    let probe = presented(Launch::SkipWithHint, URL, false).await;
    assert_eq!(probe.text(), format!("{HINT}\n{}", url_line(URL)));
    assert_eq!(probe.opened(), NOTHING);
}

#[tokio::test]
async fn terminal_never_opens_urls_without_the_https_scheme() {
    for url in [
        "file:///x",
        "file:///x?next=https://y",
        "ssh://x",
        "http://x",
        "HTTPS://x",
        "httpsx://y",
        "https:y",
        "",
    ] {
        let probe = presented(Launch::Open, url, false).await;
        assert_eq!(probe.text(), url_line(url));
        assert!(probe.opened().is_empty(), "{url}");
    }
    let probe = presented(Launch::Open, "https://x", false).await;
    assert_eq!(probe.opened(), ["https://x"]);
}

#[tokio::test]
async fn login_presents_the_url_once_and_opens_it() {
    let probe = login_until_the_url(Launch::Open, false).await;
    let text = probe.text();
    assert_eq!(probe.opened(), [printed_url(&text)]);
    assert_eq!(text.lines().count(), 1, "{text}");
}

#[tokio::test]
async fn login_prints_the_url_once_when_the_opener_fails() {
    let probe = login_until_the_url(Launch::Open, true).await;
    let text = probe.text();
    assert_eq!(probe.opened(), [printed_url(&text)]);
    assert_eq!(text.lines().count(), 1, "{text}");
}

#[tokio::test]
async fn login_skip_with_hint_prints_hint_and_url_once_and_never_opens() {
    let probe = login_until_the_url(Launch::SkipWithHint, false).await;
    hint_then_url(&probe.text());
    assert_eq!(probe.opened(), NOTHING);
}

#[tokio::test]
async fn setup_hands_the_terminal_to_the_login() {
    let dir = tempfile::tempdir().unwrap();
    let paths = prepared(dir.path());
    let probe = Probe::default();
    let mut out = Vec::new();
    until_the_url(
        &probe,
        setup(
            &paths,
            Cursor::new("\n"),
            &mut out,
            probe.terminal(Launch::Open, false),
        ),
    )
    .await;
    let wizard = String::from_utf8(out).unwrap();
    assert!(wizard.contains("Destination: "), "{wizard}");
    assert!(!wizard.contains("https://accounts.google.com/"), "{wizard}");
    let text = probe.text();
    assert_eq!(probe.opened(), [printed_url(&text)]);
    assert_eq!(text.lines().count(), 1, "{text}");
}

#[tokio::test]
async fn setup_skip_never_opens() {
    let dir = tempfile::tempdir().unwrap();
    let paths = prepared(dir.path());
    let probe = Probe::default();
    until_the_url(
        &probe,
        setup(
            &paths,
            Cursor::new("\n"),
            &mut Vec::new(),
            probe.terminal(Launch::Skip, false),
        ),
    )
    .await;
    let text = probe.text();
    printed_url(&text);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert_eq!(probe.opened(), NOTHING);
}
