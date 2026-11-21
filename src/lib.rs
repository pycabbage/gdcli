pub mod auth;
pub mod calendar;
pub mod cli;
pub mod docs;
pub mod events;
pub mod google;
pub mod meet;
pub mod output;
pub mod paths;
pub mod skills;
pub mod update;

pub use cli::{AuthCommand, CalendarCommand, Cli, Command, DocsCommand, MeetCommand};
pub use paths::Paths;

use anyhow::Result;
use cli::AccountArg;
use google::Google;
use std::io::Write;
use std::time::Duration;
use time::OffsetDateTime;

pub fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::BrokenPipe)
}

pub fn deadline_for(command: &Command) -> Option<Duration> {
    match command {
        Command::Auth(AuthCommand::Setup(_) | AuthCommand::Login(_)) => None,
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

fn terminal(
    args: cli::LoginArgs,
) -> impl yup_oauth2::authenticator_delegate::InstalledFlowDelegate {
    let launch = auth::browser::launch(args.no_browser, &|name| std::env::var_os(name));
    auth::browser::Terminal::new(launch, std::io::stdout(), auth::browser::open_url)
}

async fn execute(
    command: Command,
    resolve: &dyn Fn() -> Result<Paths>,
    out: &mut impl Write,
) -> Result<()> {
    let google =
        async |account: &AccountArg| Google::connect(&resolve()?, account.account.as_ref()).await;
    match command {
        Command::Update => update::run(out).await,
        Command::Skills { topic } => Ok(skills::print(topic, out)?),
        Command::Auth(AuthCommand::Setup(args)) => {
            auth::setup::setup(&resolve()?, std::io::stdin().lock(), out, terminal(args)).await
        }
        Command::Auth(AuthCommand::Login(args)) => {
            auth::login(&resolve()?, out, terminal(args)).await
        }
        Command::Auth(AuthCommand::List) => auth::list(&resolve()?, out).await,
        Command::Auth(AuthCommand::SetDefault { email }) => {
            auth::set_default(&resolve()?, &email, out).await
        }
        Command::Calendar(CalendarCommand::List(args)) => {
            args.check()?;
            calendar::list(&google(&args.account).await?, now(), &args, out).await
        }
        Command::Calendar(CalendarCommand::Get(args)) => {
            calendar::get(&google(&args.account).await?, &args.id, args.json, out).await
        }
        Command::Meet(MeetCommand::List(args)) => {
            args.check()?;
            meet::list(&google(&args.account).await?, now(), &args, out).await
        }
        Command::Meet(MeetCommand::Get(args)) => {
            meet::get(&google(&args.account).await?, &args.id, args.json, out).await
        }
        Command::Meet(MeetCommand::Notes(args)) => {
            meet::notes(&google(&args.account).await?, &args, out).await
        }
        Command::Meet(MeetCommand::Transcript(args)) => {
            meet::transcript(&google(&args.account).await?, &args, out).await
        }
        Command::Docs(DocsCommand::Get { id, tab, account }) => {
            docs::get(&google(&account).await?, &id, tab.as_deref(), out).await
        }
        Command::Docs(DocsCommand::Metadata { id, json, account }) => {
            docs::metadata(&google(&account).await?, &id, json, out).await
        }
    }
}

pub async fn run_with(
    cli: Cli,
    resolve: &dyn Fn() -> Result<Paths>,
    out: &mut impl Write,
) -> Result<()> {
    let deadline = deadline_for(&cli.command);
    google::with_deadline(deadline, Box::pin(execute(cli.command, resolve, out))).await
}

pub async fn run(cli: Cli) -> Result<()> {
    run_with(cli, &Paths::from_xdg, &mut std::io::stdout().lock()).await
}
