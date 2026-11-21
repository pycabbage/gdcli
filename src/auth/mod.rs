pub mod accounts;
pub mod browser;
mod token;

use crate::output;
use crate::paths::Paths;
use accounts::{Email, Stored};
use anyhow::Result;
use serde_json::json;
use std::io::Write;

pub use setup::{install_client_secret, validate_client_secret};
pub use token::{authenticator, login, secret};

pub const SCOPES: [&str; 3] = [
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/meetings.space.readonly",
    "https://www.googleapis.com/auth/drive.readonly",
];

pub async fn list(paths: &Paths, out: &mut impl Write) -> Result<()> {
    accounts::migrate(paths).await?;
    let pointer = accounts::default_account(paths).ok();
    let rows: Vec<Vec<String>> = accounts::list(paths)?
        .into_iter()
        .map(|email| {
            let default = Some(&email) == pointer.as_ref();
            vec![email.to_string(), default.to_string()]
        })
        .collect();
    Ok(output::table(out, "account\tdefault", &rows, false)?)
}

pub fn render(stored: &Stored, out: &mut impl Write) -> Result<()> {
    let status = if stored.added { "added" } else { "refreshed" };
    let record =
        json!({"account": stored.email.to_string(), "status": status, "default": stored.default});
    Ok(output::record(out, &record, false)?)
}

pub async fn set_default(paths: &Paths, email: &Email, out: &mut impl Write) -> Result<()> {
    accounts::migrate(paths).await?;
    accounts::set_default(paths, email)?;
    let record = json!({"account": email.to_string(), "default": true});
    Ok(output::record(out, &record, false)?)
}

pub mod setup;
