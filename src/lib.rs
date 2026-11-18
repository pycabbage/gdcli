pub mod api;
pub mod auth;
pub mod calendar;
pub mod cli;
pub mod docs;
pub mod http;
pub mod meet;
pub mod output;
pub mod paths;

pub use api::{Api, Endpoints};
pub use cli::{Cli, Command};
pub use paths::Paths;

use anyhow::Result;
use chrono::{DateTime, SubsecRound, Utc};

pub fn message(error: &anyhow::Error) -> String {
    if let Some(google_apis_common::Error::BadRequest(body)) = error.downcast_ref()
        && let Some(text) = body
            .pointer("/error/message")
            .and_then(|text| text.as_str())
    {
        let code = body.pointer("/error/code").and_then(|code| code.as_u64());
        let code = code.map_or_else(String::new, |code| format!(" {code}"));
        return format!("google api error{code}: {text}");
    }
    format!("{error:#}").trim().to_owned()
}

pub fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(0)
}

pub async fn run(cli: Cli) -> Result<()> {
    let paths = Paths::from_xdg()?;
    let mut out = std::io::stdout().lock();
    match cli.command {
        Command::Setup => auth::setup::setup(&paths, std::io::stdin().lock(), &mut out).await,
        Command::Login => auth::login(&paths, &mut out).await,
        Command::Meet(command) => {
            meet::run(&Api::connect(&paths).await?, command, now(), &mut out).await
        }
        Command::Calendar(command) => {
            calendar::run(&Api::connect(&paths).await?, command, now(), &mut out).await
        }
        Command::Docs(command) => docs::run(&Api::connect(&paths).await?, command, &mut out).await,
    }
}
