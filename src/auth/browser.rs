use crate::paths::Env;
use std::future::Future;
use std::io::{self, Write};
use std::pin::Pin;
use std::sync::Mutex;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;

const OPT_IN: &str = "GDCLI_OPEN_BROWSER";
const AGENT_VARIABLES: [&str; 11] = [
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
const HINT: &str = "No browser was opened because an AI agent environment was detected. Set GDCLI_OPEN_BROWSER=1 only after the user has agreed to a browser opening on their machine, never on your own; otherwise pass --no-browser and hand the printed URL to the user.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    Open,
    Skip,
    SkipWithHint,
}

pub fn launch(no_browser: bool, env: Env) -> Launch {
    if no_browser {
        Launch::Skip
    } else if env(OPT_IN).is_some_and(|value| value == "1") {
        Launch::Open
    } else if AGENT_VARIABLES
        .iter()
        .any(|name| env(name).is_some_and(|value| !value.is_empty()))
    {
        Launch::SkipWithHint
    } else {
        Launch::Open
    }
}

pub fn open_url(url: &str) -> io::Result<()> {
    open::that_detached(url)
}

pub struct Terminal<W, O> {
    launch: Launch,
    out: Mutex<W>,
    open: O,
}

impl<W, O> Terminal<W, O> {
    pub fn new(launch: Launch, out: W, open: O) -> Self {
        Self {
            launch,
            out: Mutex::new(out),
            open,
        }
    }
}

impl<W, O> InstalledFlowDelegate for Terminal<W, O>
where
    W: Write + Send + 'static,
    O: Fn(&str) -> io::Result<()> + Send + Sync + 'static,
{
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        if let Ok(mut out) = self.out.lock() {
            if self.launch == Launch::SkipWithHint {
                let _ = writeln!(out, "{HINT}");
            }
            let _ = writeln!(
                out,
                "Please direct your browser to {url} and follow the instructions displayed there."
            );
        }
        if self.launch == Launch::Open && url.starts_with("https://") {
            let _ = (self.open)(url);
        }
        Box::pin(async { Ok(String::new()) })
    }
}
