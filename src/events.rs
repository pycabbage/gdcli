use crate::cli::ListArgs;
use crate::google::Google;
use crate::output::table;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::Write;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub const CALENDAR_HEADER: &str = "id\twhen\tstart\tend\tsummary";
pub const MEET_HEADER: &str = "id\twhen\tstart\tend\tmeeting_code\tsummary";

const WINDOWS_DAYS: [i64; 4] = [7, 30, 180, 730];
const PAST_PAGE_SIZE: &str = "250";
const MEET_MIN_PAGE_SIZE: usize = 25;
const DEFAULT_LIMIT: u16 = 50;
const EVENT_TYPES: [&str; 4] = ["default", "focusTime", "outOfOffice", "fromGmail"];
const CALENDAR_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end)";
const MEET_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";

pub fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

pub fn each(event: &Value, pointer: &str, map: impl Fn(&Value) -> Value) -> Value {
    event
        .pointer(pointer)
        .and_then(Value::as_array)
        .map_or(Value::Null, |items| items.iter().map(map).collect())
}

pub fn attachments(event: &Value) -> Value {
    each(event, "/attachments", |entry| {
        json!({
            "title": text(entry, "/title"),
            "mime_type": text(entry, "/mimeType"),
            "file_id": text(entry, "/fileId"),
        })
    })
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

pub fn utc(raw: &str) -> String {
    OffsetDateTime::parse(raw, &Rfc3339).map_or_else(|_| raw.to_owned(), stamp)
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
        _ => timed(event, which).map_or_default(stamp),
    }
}

fn has_ended(event: &Value, now: OffsetDateTime) -> bool {
    instant(event, "end").is_some_and(|end| end <= now)
}

struct Window<'a> {
    from: OffsetDateTime,
    to: Option<OffsetDateTime>,
    query: Option<&'a str>,
}

async fn pages(
    google: &Google,
    fields: &str,
    page_size: &str,
    window: Window<'_>,
    mut take: impl FnMut(Value) -> bool,
) -> Result<()> {
    let (min, max) = (stamp_millis(window.from), window.to.map(stamp_millis));
    let mut token = String::new();
    loop {
        let mut request = vec![
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
            ("maxResults", page_size),
            ("fields", fields),
            ("timeMin", &min),
        ];
        request.extend(EVENT_TYPES.map(|event_type| ("eventTypes", event_type)));
        request.extend(max.as_deref().map(|max| ("timeMax", max)));
        request.extend(window.query.map(|query| ("q", query)));
        if !token.is_empty() {
            request.push(("pageToken", &token));
        }
        let mut page = google
            .json(
                &google.calendar,
                &["calendars", "primary", "events"],
                &request,
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

async fn first(
    google: &Google,
    fields: &str,
    page_size: &str,
    window: Window<'_>,
    wanted: usize,
    keep: impl Fn(&Value) -> bool,
) -> Result<Vec<Value>> {
    let mut found = Vec::with_capacity(wanted);
    if wanted == 0 {
        return Ok(found);
    }
    pages(google, fields, page_size, window, |event| {
        if text(&event, "/status") != "cancelled" && keep(&event) {
            found.push(event);
        }
        found.len() < wanted
    })
    .await?;
    Ok(found)
}

async fn past(
    google: &Google,
    now: OffsetDateTime,
    fields: &str,
    wanted: usize,
    query: Option<&str>,
    keep: impl Fn(&Value) -> bool,
) -> Result<Vec<Value>> {
    let mut tail = VecDeque::with_capacity(wanted + 1);
    for days in WINDOWS_DAYS {
        if wanted == 0 {
            break;
        }
        tail.clear();
        let window = Window {
            from: now - Duration::days(days),
            to: Some(now),
            query,
        };
        pages(google, fields, PAST_PAGE_SIZE, window, |event| {
            if text(&event, "/status") != "cancelled" && has_ended(&event, now) && keep(&event) {
                if tail.len() == wanted {
                    tail.pop_front();
                }
                tail.push_back(event);
            }
            true
        })
        .await?;
        if tail.len() == wanted {
            break;
        }
    }
    Ok(tail.into())
}

pub async fn list(
    google: &Google,
    now: OffsetDateTime,
    meet: bool,
    args: &ListArgs,
    out: &mut impl Write,
) -> Result<()> {
    let keep = |event: &Value| !meet || meeting_code(event).is_some();
    let (fields, floor, header) = if meet {
        (MEET_FIELDS, MEET_MIN_PAGE_SIZE, MEET_HEADER)
    } else {
        (CALENDAR_FIELDS, 1, CALENDAR_HEADER)
    };
    let query = args.query.as_deref();
    let events = if let Some(from) = args.from {
        let limit = usize::from(args.limit.unwrap_or(DEFAULT_LIMIT));
        let page_size = if meet { 250 } else { limit.min(250) }.to_string();
        let window = Window {
            from,
            to: args.to,
            query,
        };
        first(google, fields, &page_size, window, limit, keep).await?
    } else {
        let (past_count, upcoming_count) = (usize::from(args.past), usize::from(args.upcoming));
        let page_size = upcoming_count.max(floor).min(250).to_string();
        let window = Window {
            from: now,
            to: None,
            query,
        };
        let fresh = |event: &Value| !has_ended(event, now) && keep(event);
        let (mut past, upcoming) = tokio::try_join!(
            past(google, now, fields, past_count, query, keep),
            first(google, fields, &page_size, window, upcoming_count, fresh),
        )?;
        past.extend(upcoming);
        past
    };
    let mut rows = Vec::with_capacity(events.len());
    for event in &events {
        let when = if has_ended(event, now) {
            "past"
        } else {
            "upcoming"
        };
        let mut row = vec![
            text(event, "/id").to_owned(),
            when.to_owned(),
            render_time(event, "start"),
            render_time(event, "end"),
        ];
        row.extend(meeting_code(event).filter(|_| meet).map(str::to_owned));
        row.push(text(event, "/summary").to_owned());
        rows.push(row);
    }
    Ok(table(out, header, &rows, args.json)?)
}
