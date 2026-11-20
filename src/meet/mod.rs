mod read;
mod records;

use crate::cli::ListArgs;
use crate::events::{self, attachments, render_time, text};
use crate::google::Google;
use crate::output::{compact, record};
use anyhow::Result;
use records::{Unavailable, conference_records, conferences, event};
use serde_json::{Value, json};
use std::io::Write;
use time::OffsetDateTime;

pub use read::{notes, transcript};

const SPACE_FIELDS: &str = "config(accessType,entryPointAccess,moderation)";

pub async fn get(google: &Google, id: &str, json: bool, out: &mut impl Write) -> Result<()> {
    let (event, code) = event(google, id).await?;
    let mut unavailable = Unavailable::default();
    let space_path = ["spaces", &code];
    let space_query = [("fields", SPACE_FIELDS)];
    let (space, records) = tokio::join!(
        google.json(&google.meet, &space_path, &space_query),
        conference_records(google, &event, &code),
    );
    let space = unavailable.tolerate(space)?;
    let records = unavailable.tolerate(records)?;
    let found = conferences(google, &records, &mut unavailable).await?;
    let mut entry = compact(json!({
        "id": text(&event, "/id"),
        "summary": text(&event, "/summary"),
        "start": render_time(&event, "start"),
        "end": render_time(&event, "end"),
        "meeting_code": code,
        "meeting_uri": format!("https://meet.google.com/{code}"),
        "access_type": text(&space, "/config/accessType"),
        "entry_point_access": text(&space, "/config/entryPointAccess"),
        "moderation": text(&space, "/config/moderation"),
        "attachments": attachments(&event),
    }))
    .unwrap_or_default();
    if !records.is_null() {
        entry["conferences"] = Value::Array(found);
    }
    if let Some(reason) = unavailable.0 {
        entry["meet_api"] = format!("unavailable ({reason})").into();
    }
    Ok(record(out, &entry, json)?)
}

pub async fn list(
    google: &Google,
    now: OffsetDateTime,
    args: &ListArgs,
    out: &mut impl Write,
) -> Result<()> {
    events::list(google, now, true, args, out).await
}
