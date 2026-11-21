use crate::auth::accounts::Email;
use clap::{Args, Parser, Subcommand, ValueEnum};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const BROWSER_ENV: &str = "Environment: GDCLI_OPEN_BROWSER=1 opens the browser even in an AI agent shell (only with the user's consent).";

pub fn moment(value: &str) -> Result<OffsetDateTime, String> {
    let full = if value.len() == 10 {
        format!("{value}T00:00:00Z")
    } else {
        value.to_owned()
    };
    OffsetDateTime::parse(&full, &Rfc3339).map_err(|_| "use YYYY-MM-DD or RFC 3339".to_owned())
}

fn query_text(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err("invalid query: it must not be blank".to_owned());
    }
    Ok(value.to_owned())
}

fn event_id(value: &str) -> Result<String, String> {
    if value.is_empty() || matches!(value, "." | "..") || value.contains(['/', '?', '#', '%']) {
        return Err("invalid event id".to_owned());
    }
    Ok(value.to_owned())
}

fn tab_name(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err("invalid tab: it must not be blank".to_owned());
    }
    Ok(value.to_owned())
}

fn document_id(value: &str) -> Result<String, String> {
    let id = crate::docs::document_id(value);
    if id.is_empty() || matches!(id, "." | "..") {
        return Err("invalid document id".to_owned());
    }
    Ok(value.to_owned())
}

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(
        subcommand,
        about = "Accounts: set up the OAuth client, log in, list accounts, choose the default"
    )]
    Auth(AuthCommand),
    #[command(subcommand, about = "Google Meet meetings")]
    Meet(MeetCommand),
    #[command(subcommand, about = "Google Calendar events")]
    Calendar(CalendarCommand),
    #[command(subcommand, about = "Google Docs documents")]
    Docs(DocsCommand),
    #[command(about = "Print the agent skill of gdcli: SKILL.md, one reference, or everything")]
    Skills {
        #[arg(value_enum, help = "Reference to print; without it SKILL.md")]
        topic: Option<Topic>,
    },
    #[command(about = "Update gdcli from the latest GitHub release")]
    Update,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Topic {
    Full,
    Meet,
    Calendar,
    Docs,
    Auth,
}

#[derive(Debug, Clone, Default, Args)]
pub struct AccountArg {
    #[arg(
        short = 'a',
        long,
        value_name = "EMAIL",
        help = "Google account (email) to use instead of the default"
    )]
    pub account: Option<Email>,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(0..=1000), help = "How many past items to fetch (0 skips them)")]
    pub past: u16,
    #[arg(long, visible_alias = "future", default_value_t = 5, value_parser = clap::value_parser!(u16).range(0..=1000), help = "How many upcoming items to fetch (0 skips them)")]
    pub upcoming: u16,
    #[arg(
        long,
        value_name = "TIME",
        value_parser = moment,
        conflicts_with_all = ["past", "upcoming"],
        help = "Range mode: only events ending after this time (YYYY-MM-DD is 00:00 UTC, or RFC 3339)"
    )]
    pub from: Option<OffsetDateTime>,
    #[arg(
        long,
        value_name = "TIME",
        value_parser = moment,
        requires = "from",
        conflicts_with_all = ["past", "upcoming"],
        help = "Only events starting before this time (same formats)"
    )]
    pub to: Option<OffsetDateTime>,
    #[arg(long, value_parser = query_text, help = "Free-text Calendar search (title, description, location, people)")]
    pub query: Option<String>,
    #[arg(long, requires = "from", conflicts_with_all = ["past", "upcoming"], value_parser = clap::value_parser!(u16).range(1..=1000), help = "First N matches, oldest first (range mode, default 50)")]
    pub limit: Option<u16>,
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
    #[command(flatten)]
    pub account: AccountArg,
}

impl ListArgs {
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.from
                .zip(self.to)
                .is_none_or(|(from, to)| from.unix_timestamp() < to.unix_timestamp()),
            "invalid range: --from must be earlier than --to"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Args)]
pub struct GetArgs {
    #[arg(help = "Event id from `meet list` or `calendar list`", value_parser = event_id)]
    pub id: String,
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
    #[command(flatten)]
    pub account: AccountArg,
}

#[derive(Debug, Clone, Args)]
pub struct ReadArgs {
    #[arg(help = "Event id from `meet list` or `calendar list`", value_parser = event_id)]
    pub id: String,
    #[arg(long, value_parser = tab_name, help = "Only this tab, given by tab id (t.abc) or tab title")]
    pub tab: Option<String>,
    #[command(flatten)]
    pub account: AccountArg,
}

#[derive(Debug, Clone, Copy, Args)]
pub struct LoginArgs {
    #[arg(
        long,
        help = "Do not open a browser, only print the login URL (the default in an AI agent shell; GDCLI_OPEN_BROWSER=1 opens it there)"
    )]
    pub no_browser: bool,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    #[command(
        about = "Show setup instructions, install the OAuth client secret, then log in",
        after_help = BROWSER_ENV
    )]
    Setup(LoginArgs),
    #[command(
        about = "Log in a Google account (the first one becomes the default; a known one is refreshed)",
        after_help = BROWSER_ENV
    )]
    Login(LoginArgs),
    #[command(about = "List the logged-in accounts and mark the default")]
    List,
    #[command(about = "Make an account the default")]
    SetDefault {
        #[arg(help = "Email address of a logged-in account")]
        email: Email,
    },
}

#[derive(Debug, Subcommand)]
pub enum MeetCommand {
    #[command(about = "Meetings (events with a Google Meet link): past and upcoming")]
    List(ListArgs),
    #[command(about = "Show a meeting with its space settings, conferences and artifacts")]
    Get(GetArgs),
    #[command(
        about = "Print the Gemini notes document of a meeting as Markdown (without --tab all tabs)"
    )]
    Notes(ReadArgs),
    #[command(
        about = "Print the transcript document of a meeting as Markdown (without --tab all tabs)"
    )]
    Transcript(ReadArgs),
}

#[derive(Debug, Subcommand)]
pub enum CalendarCommand {
    #[command(about = "Calendar events: past and upcoming")]
    List(ListArgs),
    #[command(about = "Show one event")]
    Get(GetArgs),
}

#[derive(Debug, Subcommand)]
pub enum DocsCommand {
    #[command(about = "Print a Google Doc as Markdown (without --tab all tabs in one export)")]
    Get {
        #[arg(help = "Document id or docs.google.com URL", value_parser = document_id)]
        id: String,
        #[arg(long, value_parser = tab_name, help = "Only this tab, given by tab id (t.abc) or tab title")]
        tab: Option<String>,
        #[command(flatten)]
        account: AccountArg,
    },
    #[command(about = "Show the metadata of a Google Doc (title, times, owners, tabs, ...)")]
    Metadata {
        #[arg(help = "Document id or docs.google.com URL", value_parser = document_id)]
        id: String,
        #[arg(long, help = "Print JSON instead of text")]
        json: bool,
        #[command(flatten)]
        account: AccountArg,
    },
}
