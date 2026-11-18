use crate::events::{self, meeting_code, render_time, stamp_millis, text, timed};
use crate::google::{ApiError, Google};
use crate::output::field;
use anyhow::{Result, bail};
use serde_json::Value;
use std::io::Write;
use time::{Duration, OffsetDateTime};

const EVENT_FIELDS: &str =
    "id,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type)))";
const SPACE_FIELDS: &str = "config(accessType,entryPointAccess,moderation)";
const RECORDS_FIELDS: &str = "conferenceRecords(name,startTime,endTime)";
const SLACK_HOURS: i64 = 1;

fn records_filter(event: &Value, code: &str) -> String {
    let mut filter = format!("space.meeting_code = \"{code}\"");
    if let (Some(start), Some(end)) = (timed(event, "start"), timed(event, "end")) {
        let slack = Duration::hours(SLACK_HOURS);
        filter = format!(
            "{filter} AND start_time>=\"{}\" AND start_time<=\"{}\"",
            stamp_millis(start - slack),
            stamp_millis(end + slack)
        );
    }
    filter
}

fn tolerate(result: Result<Value>, unavailable: &mut Option<String>) -> Result<Value> {
    match result {
        Err(error) => match error.downcast_ref::<ApiError>() {
            Some(api) if matches!(api.status.as_u16(), 403 | 404) => {
                unavailable.get_or_insert_with(|| {
                    let message = api.message.as_deref().unwrap_or_default();
                    format!("{} {message}", api.status.as_u16())
                        .trim()
                        .to_owned()
                });
                Ok(Value::Null)
            }
            _ => Err(error),
        },
        ok => ok,
    }
}

fn words(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(" ")
}

fn participant(entry: &Value) -> String {
    let (kind, person) = [
        ("signedinUser", "user"),
        ("anonymousUser", "anonymous"),
        ("phoneUser", "phone"),
    ]
    .into_iter()
    .find_map(|(key, kind)| Some((kind, entry.get(key)?)))
    .unwrap_or(("phone", &Value::Null));
    let name = Some(text(person, "/displayName")).filter(|name| !name.is_empty());
    words(&[
        name.unwrap_or(text(person, "/user")),
        &format!("({kind})"),
        text(entry, "/earliestStartTime"),
        text(entry, "/latestEndTime"),
    ])
}

fn artifact(entry: &Value) -> String {
    let target = [
        ("/driveDestination/file", "drive"),
        ("/docsDestination/document", "doc"),
    ]
    .into_iter()
    .find(|(pointer, _)| !text(entry, pointer).is_empty())
    .map_or_else(String::new, |(pointer, label)| {
        format!("{label}:{}", text(entry, pointer))
    });
    words(&[
        text(entry, "/state"),
        text(entry, "/startTime"),
        text(entry, "/endTime"),
        &target,
    ])
}

fn items<'a>(page: &'a Value, key: &str) -> &'a [Value] {
    page.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

const ARTIFACTS: [(&str, &str, &str, &str); 3] = [
    (
        "participant",
        "participants",
        "earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser",
        "250",
    ),
    (
        "recording",
        "recordings",
        "state,startTime,endTime,driveDestination/file",
        "100",
    ),
    (
        "transcript",
        "transcripts",
        "state,startTime,endTime,docsDestination/document",
        "100",
    ),
];

async fn artifacts(google: &Google, id: &str, spec: (&str, &str, &str, &str)) -> Result<Value> {
    let (_, kind, fields, size) = spec;
    let query = [("pageSize", size), ("fields", &format!("{kind}({fields})"))];
    google
        .json(&google.meet, &["conferenceRecords", id, kind], &query)
        .await
}

type Lines = Vec<(&'static str, String)>;

async fn details(
    google: &Google,
    event: &Value,
    code: &str,
) -> Result<(Value, Lines, Option<String>)> {
    let mut unavailable = None;
    let filter = records_filter(event, code);
    let records_query = [
        ("pageSize", "5"),
        ("fields", RECORDS_FIELDS),
        ("filter", &filter),
    ];
    let space_path = ["spaces", code];
    let space_query = [("fields", SPACE_FIELDS)];
    let (space, records) = tokio::join!(
        google.json(&google.meet, &space_path, &space_query),
        google.json(&google.meet, &["conferenceRecords"], &records_query),
    );
    let space = tolerate(space, &mut unavailable)?;
    let records = tolerate(records, &mut unavailable)?;
    let mut lines = Vec::new();
    for record in items(&records, "conferenceRecords").iter().rev() {
        let name = text(record, "/name");
        let id = name.strip_prefix("conferenceRecords/").unwrap_or(name);
        let pages = tokio::join!(
            artifacts(google, id, ARTIFACTS[0]),
            artifacts(google, id, ARTIFACTS[1]),
            artifacts(google, id, ARTIFACTS[2]),
        );
        let started = words(&[id, text(record, "/startTime"), text(record, "/endTime")]);
        lines.push(("conference", started));
        for ((key, kind, ..), page) in ARTIFACTS.into_iter().zip([pages.0, pages.1, pages.2]) {
            for entry in items(&tolerate(page, &mut unavailable)?, kind) {
                let line = if key == "participant" {
                    participant(entry)
                } else {
                    artifact(entry)
                };
                lines.push((key, line));
            }
        }
    }
    if lines.is_empty() && unavailable.is_none() {
        lines.push(("conference", "none".to_owned()));
    }
    Ok((space, lines, unavailable))
}

pub async fn get(google: &Google, id: &str, out: &mut impl Write) -> Result<()> {
    let segments = ["calendars", "primary", "events", id];
    let query = [("fields", EVENT_FIELDS)];
    let event = google.json(&google.calendar, &segments, &query).await?;
    let Some(code) = meeting_code(&event) else {
        bail!("event {id} has no google meet conference");
    };
    let (space, lines, unavailable) = details(google, &event, code).await?;
    field(out, "id", text(&event, "/id"))?;
    field(out, "summary", text(&event, "/summary"))?;
    field(out, "start", &render_time(&event, "start"))?;
    field(out, "end", &render_time(&event, "end"))?;
    field(out, "meeting_code", code)?;
    field(
        out,
        "meeting_uri",
        &format!("https://meet.google.com/{code}"),
    )?;
    for (key, pointer) in [
        ("access_type", "/config/accessType"),
        ("entry_point_access", "/config/entryPointAccess"),
        ("moderation", "/config/moderation"),
    ] {
        field(out, key, text(&space, pointer))?;
    }
    for (key, value) in lines {
        field(out, key, &value)?;
    }
    if let Some(reason) = unavailable {
        field(out, "meet_api", &format!("unavailable ({reason})"))?;
    }
    Ok(())
}

pub async fn list(google: &Google, now: OffsetDateTime, out: &mut impl Write) -> Result<()> {
    events::list(google, now, true, out).await
}
