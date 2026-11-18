use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Read-only Google Workspace CLI for AI agents")]
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
    #[command(about = "Show a meeting by meeting code or conference record id")]
    Get {
        #[arg(help = "Meeting code (abc-defg-hij) or conference record id")]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum CalendarCommand {
    #[command(about = "Latest 10 events: 5 past, 5 upcoming")]
    List,
    #[command(about = "Show one event")]
    Get {
        #[arg(help = "Event id of the primary calendar")]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DocsCommand {
    #[command(about = "Print a Google Doc as Markdown")]
    Get {
        #[arg(help = "Document id or docs.google.com URL")]
        id: String,
    },
}
