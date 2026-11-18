mod client;
mod get;
mod list;
pub mod model;

pub use client::Meet;
pub use get::{Target, get, is_meeting_code, parse_target, record_id};
pub use list::{list, meeting_code};

use crate::api::Api;
use crate::cli::MeetCommand;
use anyhow::Result;
use chrono::{DateTime, Utc};
use google_apis_common::Connector;
use std::io::Write;

pub async fn run<C: Connector>(
    api: &Api<C>,
    command: MeetCommand,
    now: DateTime<Utc>,
    out: &mut impl Write,
) -> Result<()> {
    match command {
        MeetCommand::List => list(api, now, out).await,
        MeetCommand::Get { id } => get(api, &id, out).await,
    }
}
