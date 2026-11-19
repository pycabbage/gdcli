use clap::{Args, Parser, Subcommand};

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
    #[command(about = "Show setup instructions, install the OAuth client secret, then log in")]
    Setup,
    #[command(about = "Run the OAuth login flow")]
    Login,
    #[command(subcommand, about = "Google Meet meetings")]
    Meet(MeetCommand),
    #[command(subcommand, about = "Google Calendar events")]
    Calendar(CalendarCommand),
    #[command(subcommand, about = "Google Docs documents")]
    Docs(DocsCommand),
    #[command(about = "Update gdcli from the latest GitHub release")]
    Update,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(0..=1000), help = "How many past items to fetch (0 skips them)")]
    pub past: u16,
    #[arg(long, visible_alias = "future", default_value_t = 5, value_parser = clap::value_parser!(u16).range(0..=1000), help = "How many upcoming items to fetch (0 skips them)")]
    pub upcoming: u16,
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GetArgs {
    #[arg(help = "Event id from `meet list` or `calendar list`", value_parser = event_id)]
    pub id: String,
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum MeetCommand {
    #[command(about = "Meetings (events with a Google Meet link): past and upcoming")]
    List(ListArgs),
    #[command(about = "Show a meeting with its space settings, conferences and artifacts")]
    Get(GetArgs),
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
    #[command(
        about = "Print a Google Doc as Markdown (without --tab all tabs, concatenated with nothing between them)"
    )]
    Get {
        #[arg(help = "Document id or docs.google.com URL", value_parser = document_id)]
        id: String,
        #[arg(long, value_parser = tab_name, help = "Only this tab, given by tab id (t.abc) or tab title")]
        tab: Option<String>,
    },
    #[command(about = "Show the metadata of a Google Doc (title, times, owners, tabs, ...)")]
    Metadata {
        #[arg(help = "Document id or docs.google.com URL", value_parser = document_id)]
        id: String,
        #[arg(long, help = "Print JSON instead of text")]
        json: bool,
    },
}
