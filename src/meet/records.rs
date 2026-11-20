use crate::events::{meeting_code, stamp_millis, text, timed};
use crate::google::{ApiError, Google};
use crate::output::compact;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use time::Duration;

const EVENT_FIELDS: &str = "id,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))),attachments(title,mimeType,fileId)";
const RECORDS_FIELDS: &str = "conferenceRecords(name,startTime,endTime)";
const SLACK_HOURS: i64 = 1;

pub type Artifact = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    fn(&Value) -> Value,
);

pub const ARTIFACTS: [Artifact; 4] = [
    (
        "participants",
        "participants",
        "earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser",
        "250",
        participant,
    ),
    (
        "recordings",
        "recordings",
        "state,startTime,endTime,driveDestination/file",
        "100",
        recording,
    ),
    (
        "transcripts",
        "transcripts",
        "state,startTime,endTime,docsDestination/document",
        "100",
        document,
    ),
    (
        "smartNotes",
        "smart_notes",
        "state,startTime,endTime,docsDestination/document",
        "100",
        document,
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
pub struct Unavailable(pub Option<String>);

impl Unavailable {
    pub fn tolerate(&mut self, result: Result<Value>) -> Result<Value> {
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

fn document(entry: &Value) -> Value {
    json!({
        "state": text(entry, "/state"),
        "start": text(entry, "/startTime"),
        "end": text(entry, "/endTime"),
        "document_id": text(entry, "/docsDestination/document"),
    })
}

pub fn items<'a>(page: &'a Value, key: &str) -> &'a [Value] {
    page.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub fn record_id(entry: &Value) -> &str {
    let name = text(entry, "/name");
    name.strip_prefix("conferenceRecords/").unwrap_or(name)
}

pub async fn event(google: &Google, id: &str) -> Result<(Value, String)> {
    let segments = ["calendars", "primary", "events", id];
    let query = [("fields", EVENT_FIELDS)];
    let event = google.json(&google.calendar, &segments, &query).await?;
    let Some(code) = meeting_code(&event).map(str::to_owned) else {
        bail!("event {id} has no google meet conference");
    };
    Ok((event, code))
}

pub async fn conference_records(google: &Google, event: &Value, code: &str) -> Result<Value> {
    let filter = records_filter(event, code);
    let query = [
        ("pageSize", "5"),
        ("fields", RECORDS_FIELDS),
        ("filter", &filter),
    ];
    google
        .json(&google.meet, &["conferenceRecords"], &query)
        .await
}

pub async fn artifacts(google: &Google, id: &str, spec: &Artifact) -> Result<Value> {
    let (collection, _, fields, size, _) = *spec;
    let query = [
        ("pageSize", size),
        ("fields", &format!("{collection}({fields})")),
    ];
    google
        .json(&google.meet, &["conferenceRecords", id, collection], &query)
        .await
}

pub async fn conferences(
    google: &Google,
    records: &Value,
    unavailable: &mut Unavailable,
) -> Result<Vec<Value>> {
    let mut found = Vec::new();
    for entry in items(records, "conferenceRecords").iter().rev() {
        let id = record_id(entry);
        let pages = tokio::join!(
            artifacts(google, id, &ARTIFACTS[0]),
            artifacts(google, id, &ARTIFACTS[1]),
            artifacts(google, id, &ARTIFACTS[2]),
            artifacts(google, id, &ARTIFACTS[3]),
        );
        let mut conference = json!({
            "id": id,
            "start": text(entry, "/startTime"),
            "end": text(entry, "/endTime"),
        });
        let pages = [pages.0, pages.1, pages.2, pages.3];
        for ((collection, key, .., map), page) in ARTIFACTS.iter().zip(pages) {
            let page = unavailable.tolerate(page)?;
            conference[*key] = items(&page, collection).iter().map(map).collect();
        }
        found.extend(compact(conference));
    }
    Ok(found)
}
