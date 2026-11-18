use super::events::{LIST_FIELDS, past, render_time, upcoming};
use crate::api::Api;
use crate::output::row;
use anyhow::Result;
use chrono::{DateTime, Utc};
use google_apis_common::Connector;
use google_calendar3::api::Event;
use std::io::Write;

pub const WANTED: usize = 5;

pub fn write_row(out: &mut impl Write, kind: &str, id: &str, event: &Event) -> Result<()> {
    row(
        out,
        &[
            kind,
            id,
            &render_time(event.start.as_ref()),
            &render_time(event.end.as_ref()),
            event.summary.as_deref().unwrap_or_default(),
        ],
    )?;
    Ok(())
}

pub async fn list<C: Connector>(
    api: &Api<C>,
    now: DateTime<Utc>,
    out: &mut impl Write,
) -> Result<()> {
    let (past, upcoming) = tokio::try_join!(
        past(api, now, WANTED, LIST_FIELDS, |_| true),
        upcoming(api, now, WANTED, WANTED as i32, LIST_FIELDS, |_| true),
    )?;
    for (kind, events) in [("past", past), ("upcoming", upcoming)] {
        for event in &events {
            write_row(out, kind, event.id.as_deref().unwrap_or_default(), event)?;
        }
    }
    Ok(())
}
