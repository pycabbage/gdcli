use super::get::record_id;
use crate::api::Api;
use crate::calendar::{WANTED, events::upcoming, write_row};
use crate::output::row;
use anyhow::Result;
use chrono::{DateTime, Utc};
use google_apis_common::Connector;
use google_calendar3::api::Event;
use std::io::Write;

const UPCOMING_PAGE_SIZE: i32 = 25;
const UPCOMING_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";

pub fn meeting_code(event: &Event) -> Option<&str> {
    let data = event.conference_data.as_ref()?;
    let kind = data
        .conference_solution
        .as_ref()?
        .key
        .as_ref()?
        .type_
        .as_deref()?;
    if kind == "hangoutsMeet" {
        data.conference_id.as_deref()
    } else {
        None
    }
}

pub async fn list<C: Connector>(
    api: &Api<C>,
    now: DateTime<Utc>,
    out: &mut impl Write,
) -> Result<()> {
    let (records, events) = tokio::try_join!(
        api.meet.records(None, WANTED as u32),
        upcoming(
            api,
            now,
            WANTED,
            UPCOMING_PAGE_SIZE,
            UPCOMING_FIELDS,
            |event| { meeting_code(event).is_some() }
        ),
    )?;
    for record in records.iter().rev() {
        row(
            out,
            &[
                "past",
                record_id(&record.name),
                record.start_time.as_deref().unwrap_or_default(),
                record.end_time.as_deref().unwrap_or_default(),
                record.space.as_deref().unwrap_or_default(),
            ],
        )?;
    }
    for event in &events {
        write_row(
            out,
            "upcoming",
            meeting_code(event).unwrap_or_default(),
            event,
        )?;
    }
    Ok(())
}
