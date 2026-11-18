use crate::cli::ListArgs;
use crate::events::{self, meeting_code, render_time, text, utc};
use crate::google::Google;
use crate::output::{compact, record};
use anyhow::Result;
use serde_json::{Value, json};
use std::io::Write;
use time::OffsetDateTime;

const GET_FIELDS: &str = "id,status,summary,description,location,start,end,htmlLink,hangoutLink,eventType,recurringEventId,recurrence,visibility,created,updated,organizer(email,displayName),creator(email,displayName),attendees(email,displayName,responseStatus,optional),conferenceData(conferenceId,conferenceSolution(key(type)),entryPoints(entryPointType,uri))";

fn person(value: &Value) -> Value {
    json!({"name": text(value, "/displayName"), "email": text(value, "/email")})
}

fn each(event: &Value, pointer: &str, map: impl Fn(&Value) -> Value) -> Value {
    event
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(Value::Null, |items| items.iter().map(map).collect())
}

fn model(event: &Value) -> Value {
    let code = meeting_code(event);
    let link = Some(text(event, "/hangoutLink")).filter(|link| !link.is_empty());
    json!({
        "id": text(event, "/id"),
        "summary": text(event, "/summary"),
        "status": text(event, "/status"),
        "start": render_time(event, "start"),
        "end": render_time(event, "end"),
        "time_zone": text(event, "/start/timeZone"),
        "location": text(event, "/location"),
        "meeting_code": code,
        "conference_id": code.is_none().then_some(text(event, "/conferenceData/conferenceId")),
        "meeting_uri": link.map(str::to_owned).or_else(|| code.map(|code| format!("https://meet.google.com/{code}"))),
        "html_link": text(event, "/htmlLink"),
        "organizer": person(&event["organizer"]),
        "creator": person(&event["creator"]),
        "attendees": each(event, "/attendees", |attendee| {
            let mut entry = person(attendee);
            entry["response_status"] = text(attendee, "/responseStatus").into();
            entry["optional"] = attendee["optional"].as_bool().filter(|optional| *optional).into();
            entry
        }),
        "recurrence": event["recurrence"],
        "event_type": text(event, "/eventType"),
        "visibility": text(event, "/visibility"),
        "recurring_event_id": text(event, "/recurringEventId"),
        "entry_points": each(event, "/conferenceData/entryPoints", |entry| {
            json!({"type": text(entry, "/entryPointType"), "uri": text(entry, "/uri")})
        }),
        "created": utc(text(event, "/created")),
        "updated": utc(text(event, "/updated")),
        "description": text(event, "/description"),
    })
}

pub async fn list(
    google: &Google,
    now: OffsetDateTime,
    args: &ListArgs,
    out: &mut impl Write,
) -> Result<()> {
    events::list(google, now, false, args, out).await
}

pub async fn get(google: &Google, id: &str, json: bool, out: &mut impl Write) -> Result<()> {
    let segments = ["calendars", "primary", "events", id];
    let event = google
        .json(&google.calendar, &segments, &[("fields", GET_FIELDS)])
        .await?;
    let entry = compact(model(&event)).unwrap_or_default();
    Ok(record(out, &entry, json)?)
}
