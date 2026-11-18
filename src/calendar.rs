use crate::events;
use crate::google::Google;
use crate::output::flatten;
use anyhow::Result;
use std::io::Write;
use time::OffsetDateTime;

const GET_FIELDS: &str = "id,status,summary,description,location,start,end,htmlLink,hangoutLink,eventType,recurringEventId,recurrence,visibility,created,updated,organizer(email,displayName),creator(email,displayName),attendees(email,displayName,responseStatus,optional),conferenceData(conferenceId,entryPoints(entryPointType,uri))";

pub async fn list(google: &Google, now: OffsetDateTime, out: &mut impl Write) -> Result<()> {
    events::list(google, now, false, out).await
}

pub async fn get(google: &Google, id: &str, out: &mut impl Write) -> Result<()> {
    let segments = ["calendars", "primary", "events", id];
    let event = google
        .json(&google.calendar, &segments, &[("fields", GET_FIELDS)])
        .await?;
    Ok(flatten(out, "", &event)?)
}
