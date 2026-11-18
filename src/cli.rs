use clap::builder::NonEmptyStringValueParser;
use clap::{Parser, Subcommand};

fn event_id(value: &str) -> Result<String, String> {
    if value.is_empty() || matches!(value, "." | "..") || value.contains(['/', '?', '#', '%']) {
        return Err("invalid event id".to_owned());
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
}

#[derive(Debug, Subcommand)]
pub enum MeetCommand {
    #[command(about = "Latest 10 meetings: 5 past, 5 upcoming")]
    List,
    #[command(about = "Show a meeting with its space settings, conferences and artifacts")]
    Get {
        #[arg(help = "Event id from `meet list` or `calendar list`", value_parser = event_id)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum CalendarCommand {
    #[command(about = "Latest 10 events: 5 past, 5 upcoming")]
    List,
    #[command(about = "Show one event")]
    Get {
        #[arg(help = "Event id from `calendar list` or `meet list`", value_parser = event_id)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DocsCommand {
    #[command(about = "Print a Google Doc as Markdown")]
    Get {
        #[arg(help = "Document id or docs.google.com URL", value_parser = NonEmptyStringValueParser::new())]
        id: String,
    },
}
