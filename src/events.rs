use crate::google::Google;
use crate::output::row;
use anyhow::Result;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::Write;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub const CALENDAR_HEADER: &str = "id\twhen\tstart\tend\tsummary";
pub const MEET_HEADER: &str = "id\twhen\tstart\tend\tmeeting_code\tsummary";

const WANTED: usize = 5;
const WINDOWS_DAYS: [i64; 4] = [7, 30, 180, 730];
const PAST_PAGE_SIZE: &str = "250";
const MEET_UPCOMING_PAGE_SIZE: &str = "25";
const EVENT_TYPES: [&str; 4] = ["default", "focusTime", "outOfOffice", "fromGmail"];
const CALENDAR_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end)";
const MEET_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";

pub fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

pub fn meeting_code(event: &Value) -> Option<&str> {
    let code = text(event, "/conferenceData/conferenceId");
    (text(event, "/conferenceData/conferenceSolution/key/type") == "hangoutsMeet"
        && !code.is_empty())
    .then_some(code)
}

pub fn stamp(moment: OffsetDateTime) -> String {
    OffsetDateTime::from_unix_timestamp(moment.unix_timestamp())
        .ok()
        .and_then(|utc| utc.format(&Rfc3339).ok())
        .unwrap_or_default()
}

pub fn stamp_millis(moment: OffsetDateTime) -> String {
    format!("{}.000Z", stamp(moment).trim_end_matches('Z'))
}

pub fn timed(event: &Value, which: &str) -> Option<OffsetDateTime> {
    let date_time = event.get(which)?.get("dateTime")?.as_str()?;
    OffsetDateTime::parse(date_time, &Rfc3339).ok()
}

fn instant(event: &Value, which: &str) -> Option<OffsetDateTime> {
    timed(event, which).or_else(|| {
        let date = event.get(which)?.get("date")?.as_str()?;
        OffsetDateTime::parse(&format!("{date}T00:00:00Z"), &Rfc3339).ok()
    })
}

pub fn render_time(event: &Value, which: &str) -> String {
    match event.get(which).and_then(|moment| moment.get("date")) {
        Some(Value::String(date)) => date.clone(),
        _ => instant(event, which).map(stamp).unwrap_or_default(),
    }
}

fn has_ended(event: &Value, now: OffsetDateTime) -> bool {
    instant(event, "end").is_some_and(|end| end <= now)
}

async fn pages(
    google: &Google,
    fields: &str,
    page_size: &str,
    from: OffsetDateTime,
    to: Option<OffsetDateTime>,
    mut take: impl FnMut(Value) -> bool,
) -> Result<()> {
    let (min, max) = (stamp_millis(from), to.map(stamp_millis));
    let mut token = String::new();
    loop {
        let mut query = vec![
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
            ("maxResults", page_size),
            ("fields", fields),
            ("timeMin", &min),
        ];
        query.extend(EVENT_TYPES.map(|event_type| ("eventTypes", event_type)));
        query.extend(max.as_deref().map(|max| ("timeMax", max)));
        if !token.is_empty() {
            query.push(("pageToken", &token));
        }
        let mut page = google
            .json(
                &google.calendar,
                &["calendars", "primary", "events"],
                &query,
            )
            .await?;
        let Value::Array(items) = page["items"].take() else {
            return Ok(());
        };
        for event in items {
            if !take(event) {
                return Ok(());
            }
        }
        text(&page, "/nextPageToken").clone_into(&mut token);
        if token.is_empty() {
            return Ok(());
        }
    }
}

async fn upcoming(
    google: &Google,
    now: OffsetDateTime,
    fields: &str,
    page_size: &str,
    keep: impl Fn(&Value) -> bool,
) -> Result<Vec<Value>> {
    let mut found = Vec::with_capacity(WANTED);
    pages(google, fields, page_size, now, None, |event| {
        if text(&event, "/status") != "cancelled" && !has_ended(&event, now) && keep(&event) {
            found.push(event);
        }
        found.len() < WANTED
    })
    .await?;
    Ok(found)
}

async fn past(
    google: &Google,
    now: OffsetDateTime,
    fields: &str,
    keep: impl Fn(&Value) -> bool,
) -> Result<Vec<Value>> {
    let mut tail = VecDeque::with_capacity(WANTED + 1);
    for days in WINDOWS_DAYS {
        tail.clear();
        let from = now - Duration::days(days);
        pages(google, fields, PAST_PAGE_SIZE, from, Some(now), |event| {
            if text(&event, "/status") != "cancelled" && has_ended(&event, now) && keep(&event) {
                if tail.len() == WANTED {
                    tail.pop_front();
                }
                tail.push_back(event);
            }
            true
        })
        .await?;
        if tail.len() == WANTED {
            break;
        }
    }
    Ok(tail.into())
}

pub async fn list(
    google: &Google,
    now: OffsetDateTime,
    meet: bool,
    out: &mut impl Write,
) -> Result<()> {
    let keep = |event: &Value| !meet || meeting_code(event).is_some();
    let (fields, page_size, header) = if meet {
        (MEET_FIELDS, MEET_UPCOMING_PAGE_SIZE, MEET_HEADER)
    } else {
        (CALENDAR_FIELDS, "5", CALENDAR_HEADER)
    };
    let (past, upcoming) = tokio::try_join!(
        past(google, now, fields, keep),
        upcoming(google, now, fields, page_size, keep),
    )?;
    writeln!(out, "{header}")?;
    for (when, events) in [("past", &past), ("upcoming", &upcoming)] {
        for event in events {
            let (start, end) = (render_time(event, "start"), render_time(event, "end"));
            let mut columns = vec![text(event, "/id"), when, &start, &end];
            columns.extend(meeting_code(event).filter(|_| meet));
            columns.push(text(event, "/summary"));
            row(out, &columns)?;
        }
    }
    Ok(())
}
