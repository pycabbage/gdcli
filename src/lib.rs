pub mod auth;
pub mod calendar;
pub mod cli;
pub mod docs;
pub mod events;
pub mod google;
pub mod meet;
pub mod output;
pub mod paths;

pub use cli::{CalendarCommand, Cli, Command, DocsCommand, MeetCommand};
pub use paths::Paths;

use anyhow::Result;
use google::Google;
use std::time::Duration;
use time::OffsetDateTime;

pub fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::BrokenPipe)
}

pub fn deadline_for(command: &Command) -> Option<Duration> {
    match command {
        Command::Setup | Command::Login => None,
        _ => Some(google::DEADLINE),
    }
}

pub fn message(error: &anyhow::Error) -> String {
    format!("{error:#}").trim().to_owned()
}

pub fn now() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::now_utc().unix_timestamp())
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
}

async fn execute(command: Command, paths: &Paths) -> Result<()> {
    let mut out = std::io::stdout().lock();
    let google = async || Google::connect(paths).await;
    match command {
        Command::Setup => auth::setup::setup(paths, std::io::stdin().lock(), &mut out).await,
        Command::Login => auth::login(paths, &mut out).await,
        Command::Calendar(CalendarCommand::List) => {
            calendar::list(&google().await?, now(), &mut out).await
        }
        Command::Calendar(CalendarCommand::Get { id }) => {
            calendar::get(&google().await?, &id, &mut out).await
        }
        Command::Meet(MeetCommand::List) => meet::list(&google().await?, now(), &mut out).await,
        Command::Meet(MeetCommand::Get { id }) => meet::get(&google().await?, &id, &mut out).await,
        Command::Docs(DocsCommand::Get { id }) => docs::get(&google().await?, &id, &mut out).await,
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    let paths = Paths::from_xdg()?;
    let deadline = deadline_for(&cli.command);
    google::with_deadline(deadline, Box::pin(execute(cli.command, &paths))).await
}
