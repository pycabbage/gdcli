use crate::cli::ListArgs;
use crate::events::{self, meeting_code, render_time, stamp_millis, text, timed};
use crate::google::{ApiError, Google};
use crate::output::{compact, record};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::io::Write;
use time::{Duration, OffsetDateTime};

const EVENT_FIELDS: &str =
    "id,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type)))";
const SPACE_FIELDS: &str = "config(accessType,entryPointAccess,moderation)";
const RECORDS_FIELDS: &str = "conferenceRecords(name,startTime,endTime)";
const SLACK_HOURS: i64 = 1;

type Artifact = (
    &'static str,
    &'static str,
    &'static str,
    fn(&Value) -> Value,
);

const ARTIFACTS: [Artifact; 3] = [
    (
        "participants",
        "earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser",
        "250",
        participant,
    ),
    (
        "recordings",
        "state,startTime,endTime,driveDestination/file",
        "100",
        recording,
    ),
    (
        "transcripts",
        "state,startTime,endTime,docsDestination/document",
        "100",
        transcript,
    ),
];

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

#[derive(Default)]
struct Unavailable(Option<String>);

impl Unavailable {
    fn tolerate(&mut self, result: Result<Value>) -> Result<Value> {
        match result {
            Err(error) => match error.downcast_ref::<ApiError>() {
                Some(api) if matches!(api.status.as_u16(), 403 | 404) => {
                    let message = api.message.as_deref().unwrap_or_default();
                    self.0.get_or_insert_with(|| {
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
}

fn participant(entry: &Value) -> Value {
    let (kind, person) = [
        ("signedinUser", "user"),
        ("anonymousUser", "anonymous"),
        ("phoneUser", "phone"),
    ]
    .into_iter()
    .find_map(|(key, kind)| Some((kind, entry.get(key)?)))
    .unwrap_or(("phone", &Value::Null));
    let name = Some(text(person, "/displayName")).filter(|name| !name.is_empty());
    json!({
        "name": name.unwrap_or(text(person, "/user")),
        "kind": kind,
        "start": text(entry, "/earliestStartTime"),
        "end": text(entry, "/latestEndTime"),
    })
}

fn recording(entry: &Value) -> Value {
    json!({
        "state": text(entry, "/state"),
        "start": text(entry, "/startTime"),
        "end": text(entry, "/endTime"),
        "drive_file": text(entry, "/driveDestination/file"),
    })
}

fn transcript(entry: &Value) -> Value {
    json!({
        "state": text(entry, "/state"),
        "start": text(entry, "/startTime"),
        "end": text(entry, "/endTime"),
        "document_id": text(entry, "/docsDestination/document"),
    })
}

fn items<'a>(page: &'a Value, key: &str) -> &'a [Value] {
    page.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

async fn artifacts(google: &Google, id: &str, spec: &Artifact) -> Result<Value> {
    let (kind, fields, size, _) = *spec;
    let query = [("pageSize", size), ("fields", &format!("{kind}({fields})"))];
    google
        .json(&google.meet, &["conferenceRecords", id, kind], &query)
        .await
}

async fn conferences(
    google: &Google,
    records: &Value,
    unavailable: &mut Unavailable,
) -> Result<Vec<Value>> {
    let mut found = Vec::new();
    for entry in items(records, "conferenceRecords").iter().rev() {
        let name = text(entry, "/name");
        let id = name.strip_prefix("conferenceRecords/").unwrap_or(name);
        let pages = tokio::join!(
            artifacts(google, id, &ARTIFACTS[0]),
            artifacts(google, id, &ARTIFACTS[1]),
            artifacts(google, id, &ARTIFACTS[2]),
        );
        let mut conference = json!({
            "id": id,
            "start": text(entry, "/startTime"),
            "end": text(entry, "/endTime"),
        });
        for ((kind, .., map), page) in ARTIFACTS.iter().zip([pages.0, pages.1, pages.2]) {
            let page = unavailable.tolerate(page)?;
            conference[*kind] = items(&page, kind).iter().map(map).collect();
        }
        found.extend(compact(conference));
    }
    Ok(found)
}

pub async fn get(google: &Google, id: &str, json: bool, out: &mut impl Write) -> Result<()> {
    let segments = ["calendars", "primary", "events", id];
    let query = [("fields", EVENT_FIELDS)];
    let event = google.json(&google.calendar, &segments, &query).await?;
    let Some(code) = meeting_code(&event) else {
        bail!("event {id} has no google meet conference");
    };
    let mut unavailable = Unavailable::default();
    let filter = records_filter(&event, code);
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
