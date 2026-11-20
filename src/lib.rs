pub mod auth;
pub mod calendar;
pub mod cli;
pub mod docs;
pub mod events;
pub mod google;
pub mod meet;
pub mod output;
pub mod paths;
pub mod update;

pub use cli::{AuthCommand, CalendarCommand, Cli, Command, DocsCommand, MeetCommand};
pub use paths::Paths;

use anyhow::Result;
use cli::AccountArg;
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
        Command::Auth(AuthCommand::Setup | AuthCommand::Login) => None,
        _ => Some(google::DEADLINE),
    }
}

pub fn message(error: &anyhow::Error) -> String {
    format!("{error:#}").trim().to_owned()
}

pub fn now() -> OffsetDateTime {
    let now = OffsetDateTime::now_utc();
    now.replace_nanosecond(0).unwrap_or(now)
}

async fn execute(command: Command, resolve: &dyn Fn() -> Result<Paths>) -> Result<()> {
    let mut out = std::io::stdout().lock();
    let google =
        async |account: &AccountArg| Google::connect(&resolve()?, account.account.as_ref()).await;
    match command {
        Command::Update => update::run(&mut out).await,
        Command::Auth(AuthCommand::Setup) => {
            auth::setup::setup(&resolve()?, std::io::stdin().lock(), &mut out).await
        }
        Command::Auth(AuthCommand::Login) => auth::login(&resolve()?, &mut out).await,
        Command::Auth(AuthCommand::List) => auth::list(&resolve()?, &mut out).await,
        Command::Auth(AuthCommand::SetDefault { email }) => {
            auth::set_default(&resolve()?, &email, &mut out).await
        }
        Command::Calendar(CalendarCommand::List(args)) => {
            calendar::list(&google(&args.account).await?, now(), &args, &mut out).await
        }
        Command::Calendar(CalendarCommand::Get(args)) => {
            calendar::get(&google(&args.account).await?, &args.id, args.json, &mut out).await
        }
        Command::Meet(MeetCommand::List(args)) => {
            meet::list(&google(&args.account).await?, now(), &args, &mut out).await
        }
        Command::Meet(MeetCommand::Get(args)) => {
            meet::get(&google(&args.account).await?, &args.id, args.json, &mut out).await
        }
        Command::Docs(DocsCommand::Get { id, tab, account }) => {
            docs::get(&google(&account).await?, &id, tab.as_deref(), &mut out).await
        }
        Command::Docs(DocsCommand::Metadata { id, json, account }) => {
            docs::metadata(&google(&account).await?, &id, json, &mut out).await
        }
    }
}

pub async fn run_with(cli: Cli, resolve: &dyn Fn() -> Result<Paths>) -> Result<()> {
    let deadline = deadline_for(&cli.command);
    google::with_deadline(deadline, Box::pin(execute(cli.command, resolve))).await
}

pub async fn run(cli: Cli) -> Result<()> {
    run_with(cli, &Paths::from_xdg).await
}
