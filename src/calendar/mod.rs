pub mod events;
mod get;
mod list;

pub use get::{GET_FIELDS, get, render};
pub use list::{WANTED, list, write_row};

use crate::api::Api;
use crate::cli::CalendarCommand;
use anyhow::Result;
use chrono::{DateTime, Utc};
use google_apis_common::Connector;
use std::io::Write;

pub async fn run<C: Connector>(
    api: &Api<C>,
    command: CalendarCommand,
    now: DateTime<Utc>,
    out: &mut impl Write,
) -> Result<()> {
    match command {
        CalendarCommand::List => list(api, now, out).await,
        CalendarCommand::Get { id } => get(api, &id, out).await,
    }
}
