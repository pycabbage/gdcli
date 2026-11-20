mod common;

use common::{
    CODE, EVENT_FIELDS, EVENT_TYPES, EVENTS_PATH, Harness, RECORDS_FIELDS, at, event, meet_event,
    now, records_filter, text,
};
use gdcli::meet;
use serde_json::{Value, json};
use std::collections::HashMap;

const HEADER: &str = "id\twhen\tstart\tend\tmeeting_code\tsummary";
const LIST_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";
const SPACE_FIELDS: &str = "config(accessType,entryPointAccess,moderation)";

async fn list_output(harness: &Harness) -> String {
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &common::args(), &mut out)
        .await
        .unwrap();
    text(out)
}

fn rows(output: &str) -> Vec<Vec<&str>> {
    output
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect()
}

fn past_windows(requests: &[HashMap<String, String>]) -> Vec<String> {
    requests
        .iter()
        .filter(|query| query.contains_key("timeMax"))
        .map(|query| query["timeMin"].clone())
        .collect()
}

fn at_day(id: &str, code: &str, day: u32, hour: u32) -> Value {
    meet_event(
        id,
        code,
        &format!("2026-11-{day:02}T{hour:02}:00:00Z"),
        &format!("2026-11-{day:02}T{hour:02}:30:00Z"),
    )
}

#[tokio::test]
async fn list_shows_only_meet_events_with_the_event_id_first() {
    let harness = Harness::start().await;
    let mut other = at_day("other", "zzz", 16, 3);
    other["conferenceData"]["conferenceSolution"]["key"]["type"] = json!("addOn");
    let mut dead = at_day("dead", "ddd-dddd-ddd", 17, 4);
    dead["status"] = json!("cancelled");
    harness
        .mount_calendar(vec![
            event("plain-past", "2026-11-14T01:00:00Z", "2026-11-14T02:00:00Z"),
            at_day("m1", "aaa-bbbb-ccc", 15, 1),
            other,
            at_day("m2", "ddd-eeee-fff", 16, 1),
            dead,
            meet_event(
                "live",
                "lll-live-lll",
                "2026-11-17T23:00:00Z",
                "2026-11-18T01:00:00Z",
            ),
            event("plain-next", "2026-11-19T01:00:00Z", "2026-11-19T02:00:00Z"),
            at_day("m3", "ggg-hhhh-iii", 20, 1),
            at_day("m4", "jjj-kkkk-lll", 22, 1),
        ])
        .await;
    let output = list_output(&harness).await;
    assert_eq!(
        output,
        format!(
            "{HEADER}\n\
             m1\tpast\t2026-11-15T01:00:00Z\t2026-11-15T01:30:00Z\taaa-bbbb-ccc\tEvent m1\n\
             m2\tpast\t2026-11-16T01:00:00Z\t2026-11-16T01:30:00Z\tddd-eeee-fff\tEvent m2\n\
             live\tupcoming\t2026-11-17T23:00:00Z\t2026-11-18T01:00:00Z\tlll-live-lll\tEvent live\n\
             m3\tupcoming\t2026-11-20T01:00:00Z\t2026-11-20T01:30:00Z\tggg-hhhh-iii\tEvent m3\n\
             m4\tupcoming\t2026-11-22T01:00:00Z\t2026-11-22T01:30:00Z\tjjj-kkkk-lll\tEvent m4\n"
        )
    );
    for line in output.lines() {
        assert_eq!(
            line.split('\t').count(),
            HEADER.split('\t').count(),
            "{line}"
        );
    }
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 5);
    for types in harness.query_values(EVENTS_PATH, "eventTypes").await {
        assert_eq!(types, EVENT_TYPES);
    }
    for query in &requests {
        assert_eq!(query["fields"], LIST_FIELDS);
        assert_eq!(query["singleEvents"], "true");
        assert_eq!(query["orderBy"], "startTime");
        let expected = if query.contains_key("timeMax") {
            "250"
        } else {
            "25"
        };
        assert_eq!(query["maxResults"], expected);
    }
}

#[tokio::test]
async fn list_applies_the_meet_predicate_before_the_past_ring_buffer() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (0..8)
        .map(|index| at_day(&format!("m{index}"), "aaa-bbbb-ccc", 12, index))
        .collect();
    events.extend((0..20).map(|index| {
        event(
            &format!("n{index:02}"),
            &format!("2026-11-14T{index:02}:00:00Z"),
            &format!("2026-11-14T{index:02}:30:00Z"),
        )
    }));
    harness.mount_calendar(events).await;
    let output = list_output(&harness).await;
    let ids: Vec<&str> = rows(&output).iter().map(|row| row[0]).collect();
    assert_eq!(ids, ["m3", "m4", "m5", "m6", "m7"]);
}

#[tokio::test]
async fn list_widens_the_past_window_until_five_meet_events_are_found() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (0..30)
        .map(|index| {
            event(
                &format!("n{index:02}"),
                &format!("2026-11-15T{index:02}:00:00Z", index = index % 24),
                &format!("2026-11-15T{index:02}:30:00Z", index = index % 24),
            )
        })
        .collect();
    events.extend((1..=5).map(|index| {
        meet_event(
            &format!("old{index}"),
            "aaa-bbbb-ccc",
            &format!("2026-10-{:02}T10:00:00Z", index + 5),
            &format!("2026-10-{:02}T11:00:00Z", index + 5),
        )
    }));
    harness.mount_calendar(events).await;
    let output = list_output(&harness).await;
    let ids: Vec<&str> = rows(&output).iter().map(|row| row[0]).collect();
    assert_eq!(ids, ["old1", "old2", "old3", "old4", "old5"]);
    let windows = past_windows(&harness.requests(EVENTS_PATH).await);
    assert_eq!(
        windows,
        [
            "2026-11-11T00:00:00.000Z".to_owned(),
            common::millis(at("2026-10-19T00:00:00Z")),
            common::millis(at("2026-05-22T00:00:00Z")),
        ]
    );
}

#[tokio::test]
async fn list_pages_upcoming_until_five_meet_events_are_found() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (0..30)
        .map(|index| {
            event(
                &format!("n{index}"),
                &format!("2027-01-01T{:02}:{:02}:00Z", index / 6, (index % 6) * 10),
                &format!("2027-01-01T{:02}:{:02}:30Z", index / 6, (index % 6) * 10),
            )
        })
        .collect();
    events.extend((0..5).map(|index| {
        meet_event(
            &format!("m{index}"),
            &format!("abc-defg-hi{index}"),
            &format!("2027-02-01T0{index}:00:00Z"),
            &format!("2027-02-01T0{index}:30:00Z"),
        )
    }));
    harness.mount_calendar(events).await;
    let output = list_output(&harness).await;
    let mut lines = output.lines();
    assert_eq!(lines.next(), Some(HEADER));
    assert_eq!(lines.clone().count(), 5);
    assert!(lines.all(|line| line.contains("\tupcoming\t")));
    let upcoming = harness
        .requests(EVENTS_PATH)
        .await
        .into_iter()
        .filter(|query| !query.contains_key("timeMax"))
        .count();
    assert_eq!(upcoming, 2);
}

#[tokio::test]
async fn list_drops_ended_events_from_upcoming_so_nothing_prints_twice() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            EVENTS_PATH,
            json!({"items": [
                {"id": "allday", "status": "confirmed", "summary": "Offsite",
                 "start": {"date": "2026-11-17"}, "end": {"date": "2026-11-18"},
                 "conferenceData": {"conferenceId": "aaa-bbbb-ccc", "conferenceSolution": {"key": {"type": "hangoutsMeet"}}}}
            ]}),
        )
        .await;
    let output = list_output(&harness).await;
    assert_eq!(
        output,
        format!("{HEADER}\nallday\tpast\t2026-11-17\t2026-11-18\taaa-bbbb-ccc\tOffsite\n")
    );
}

#[tokio::test]
async fn list_prints_only_the_header_when_there_are_no_meetings() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![event(
            "plain",
            "2026-11-19T01:00:00Z",
            "2026-11-19T02:00:00Z",
        )])
        .await;
    assert_eq!(list_output(&harness).await, format!("{HEADER}\n"));
}

#[tokio::test]
async fn list_prints_nothing_when_the_api_fails() {
    let harness = Harness::start().await;
    let mut out = Vec::new();
    assert!(
        meet::list(&harness.api, now(), &common::args(), &mut out)
            .await
            .is_err()
    );
    assert_eq!(out, b"");
}

#[tokio::test]
async fn list_never_calls_the_meet_api() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![at_day("m1", CODE, 20, 1)])
        .await;
    list_output(&harness).await;
    for request in harness.server.received_requests().await.unwrap() {
        assert!(
            request.url.path().starts_with("/calendar/"),
            "{}",
            request.url
        );
    }
}

async fn mount_meeting(harness: &Harness) {
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/evt1"),
            meet_event("evt1", CODE, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
        )
        .await;
    harness
        .mount_json(
            &format!("/v2/spaces/{CODE}"),
            json!({"config": {"accessType": "TRUSTED", "entryPointAccess": "ALL", "moderation": "OFF"}}),
        )
        .await;
}

async fn mount_artifacts(harness: &Harness, record: &str, participants: Value) {
    harness
        .mount_json(
            &format!("/v2/conferenceRecords/{record}/participants"),
            json!({"participants": participants}),
        )
        .await;
    harness
        .mount_json(
            &format!("/v2/conferenceRecords/{record}/recordings"),
            json!({"recordings": [{"state": "FILE_GENERATED", "startTime": "2026-11-17T01:00:00Z", "endTime": "2026-11-17T02:00:00Z", "driveDestination": {"file": format!("FILE-{record}")}}]}),
        )
        .await;
    harness
        .mount_json(
            &format!("/v2/conferenceRecords/{record}/transcripts"),
            json!({"transcripts": [{"state": "ENDED", "docsDestination": {"document": format!("DOC-{record}")}}]}),
        )
        .await;
    harness
        .mount_json(
            &format!("/v2/conferenceRecords/{record}/smartNotes"),
            json!({}),
        )
        .await;
}

async fn get_output(harness: &Harness, id: &str) -> anyhow::Result<String> {
    let mut out = Vec::new();
    meet::get(&harness.api, id, false, &mut out).await?;
    Ok(text(out))
}

async fn assert_meet_requests(harness: &Harness) {
    let event = harness.requests(&format!("{EVENTS_PATH}/evt1")).await;
    assert_eq!(event.len(), 1);
    assert_eq!(event[0]["fields"], EVENT_FIELDS);
    let space = harness.requests(&format!("/v2/spaces/{CODE}")).await;
    assert_eq!(space.len(), 1);
    assert_eq!(space[0]["fields"], SPACE_FIELDS);
    assert!(!space[0].contains_key("pageSize"));
    let records = harness.requests("/v2/conferenceRecords").await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["filter"], records_filter());
    assert_eq!(records[0]["pageSize"], "5");
    assert_eq!(records[0]["fields"], RECORDS_FIELDS);
    for record in ["REC1", "REC2"] {
        for (kind, fields, size) in [
            (
                "participants",
                "participants(earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser)",
                "250",
            ),
            (
                "recordings",
                "recordings(state,startTime,endTime,driveDestination/file)",
                "100",
            ),
            (
                "transcripts",
                "transcripts(state,startTime,endTime,docsDestination/document)",
                "100",
            ),
            (
                "smartNotes",
                "smartNotes(state,startTime,endTime,docsDestination/document)",
                "100",
            ),
        ] {
            let requests = harness
                .requests(&format!("/v2/conferenceRecords/{record}/{kind}"))
                .await;
            assert_eq!(requests.len(), 1, "{record} {kind}");
            assert_eq!(requests[0]["fields"], fields);
            assert_eq!(requests[0]["pageSize"], size);
        }
    }
}

#[tokio::test]
async fn get_prints_event_space_and_every_conference_oldest_first() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness
        .mount_records(
        records_filter(),
        json!([
            {"name": "conferenceRecords/REC2", "startTime": "2026-11-17T01:30:00Z"},
            {"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z", "endTime": "2026-11-17T01:20:00Z"}
        ]),
    )
    .await;
    mount_artifacts(
        &harness,
        "REC1",
        json!([
            {"signedinUser": {"user": "users/1", "displayName": "Alice"}, "earliestStartTime": "2026-11-17T01:00:00Z", "latestEndTime": "2026-11-17T01:20:00Z"},
            {"anonymousUser": {"displayName": "Guest"}, "earliestStartTime": "2026-11-17T01:05:00Z"}
        ]),
    )
    .await;
    mount_artifacts(
        &harness,
        "REC2",
        json!([{"phoneUser": {"displayName": "+81 90"}}]),
    )
    .await;
    let text = get_output(&harness, "evt1").await.unwrap();
    assert_eq!(
        text,
        "id: evt1\nsummary: Event evt1\nstart: 2026-11-17T01:00:00Z\nend: 2026-11-17T02:00:00Z\n\
         meeting_code: abc-defg-hij\nmeeting_uri: https://meet.google.com/abc-defg-hij\n\
         access_type: TRUSTED\nentry_point_access: ALL\nmoderation: OFF\n\
         conference: id=REC1 start=2026-11-17T01:00:00Z end=2026-11-17T01:20:00Z\n\
         participant: name=Alice kind=user start=2026-11-17T01:00:00Z end=2026-11-17T01:20:00Z\n\
         participant: name=Guest kind=anonymous start=2026-11-17T01:05:00Z\n\
         recording: state=FILE_GENERATED start=2026-11-17T01:00:00Z end=2026-11-17T02:00:00Z drive_file=FILE-REC1\n\
         transcript: state=ENDED document_id=DOC-REC1\n\
         conference: id=REC2 start=2026-11-17T01:30:00Z\n\
         participant: name=\"+81 90\" kind=phone\n\
         recording: state=FILE_GENERATED start=2026-11-17T01:00:00Z end=2026-11-17T02:00:00Z drive_file=FILE-REC2\n\
         transcript: state=ENDED document_id=DOC-REC2\n"
    );
    assert_meet_requests(&harness).await;
    let mut out = Vec::new();
    meet::get(&harness.api, "evt1", true, &mut out)
        .await
        .unwrap();
    let json = common::text(out);
    assert_eq!(
        json,
        format!(
            "{}\n",
            json!({
                "id": "evt1", "summary": "Event evt1",
                "start": "2026-11-17T01:00:00Z", "end": "2026-11-17T02:00:00Z",
                "meeting_code": "abc-defg-hij", "meeting_uri": "https://meet.google.com/abc-defg-hij",
                "access_type": "TRUSTED", "entry_point_access": "ALL", "moderation": "OFF",
                "conferences": [
                    {"id": "REC1", "start": "2026-11-17T01:00:00Z", "end": "2026-11-17T01:20:00Z",
                     "participants": [
                        {"name": "Alice", "kind": "user", "start": "2026-11-17T01:00:00Z", "end": "2026-11-17T01:20:00Z"},
                        {"name": "Guest", "kind": "anonymous", "start": "2026-11-17T01:05:00Z"}],
                     "recordings": [{"state": "FILE_GENERATED", "start": "2026-11-17T01:00:00Z", "end": "2026-11-17T02:00:00Z", "drive_file": "FILE-REC1"}],
                     "transcripts": [{"state": "ENDED", "document_id": "DOC-REC1"}]},
                    {"id": "REC2", "start": "2026-11-17T01:30:00Z",
                     "participants": [{"name": "+81 90", "kind": "phone"}],
                     "recordings": [{"state": "FILE_GENERATED", "start": "2026-11-17T01:00:00Z", "end": "2026-11-17T02:00:00Z", "drive_file": "FILE-REC2"}],
                     "transcripts": [{"state": "ENDED", "document_id": "DOC-REC2"}]}
                ]
            })
        )
    );
    common::assert_same_information(&text, &json);
}

const NOTE_FIELDS: &str = "smartNotes(state,startTime,endTime,docsDestination/document)";

async fn mount_other_artifacts(harness: &Harness, record: &str) {
    for kind in ["participants", "recordings", "transcripts"] {
        harness.mount_collection(record, kind, json!([])).await;
    }
}

async fn mount_two_records(harness: &Harness) {
    mount_meeting(harness).await;
    harness
        .mount_records(
            records_filter(),
            json!([
                {"name": "conferenceRecords/REC2", "startTime": "2026-11-17T01:30:00Z"},
                {"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z"}
            ]),
        )
        .await;
    mount_other_artifacts(harness, "REC1").await;
    mount_other_artifacts(harness, "REC2").await;
}

#[tokio::test]
async fn smart_notes_appear_per_conference_with_the_exact_request() {
    let harness = Harness::start().await;
    mount_two_records(&harness).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([
                {"state": "FILE_GENERATED", "startTime": "2026-11-17T01:00:09Z", "endTime": "2026-11-17T01:20:00Z", "docsDestination": {"document": "NOTES1"}},
                {"state": "ENDED", "startTime": "2026-11-17T01:21:00Z"}
            ]),
        )
        .await;
    harness
        .mount_collection(
            "REC2",
            "smartNotes",
            json!([{"state": "STARTED", "startTime": "2026-11-17T01:31:00Z"}]),
        )
        .await;
    let text = get_output(&harness, "evt1").await.unwrap();
    assert!(
        text.ends_with(
            "conference: id=REC1 start=2026-11-17T01:00:00Z\n\
             smart_note: state=FILE_GENERATED start=2026-11-17T01:00:09Z end=2026-11-17T01:20:00Z document_id=NOTES1\n\
             smart_note: state=ENDED start=2026-11-17T01:21:00Z\n\
             conference: id=REC2 start=2026-11-17T01:30:00Z\n\
             smart_note: state=STARTED start=2026-11-17T01:31:00Z\n"
        ),
        "{text}"
    );
    for record in ["REC1", "REC2"] {
        let requests = harness
            .requests(&format!("/v2/conferenceRecords/{record}/smartNotes"))
            .await;
        assert_eq!(requests.len(), 1, "{record}");
        assert_eq!(requests[0]["fields"], NOTE_FIELDS);
        assert_eq!(requests[0]["pageSize"], "100");
    }
    let mut out = Vec::new();
    meet::get(&harness.api, "evt1", true, &mut out)
        .await
        .unwrap();
    let json = common::text(out);
    assert!(
        json.contains(r#""smart_notes":[{"state":"FILE_GENERATED","start":"2026-11-17T01:00:09Z","end":"2026-11-17T01:20:00Z","document_id":"NOTES1"},{"state":"ENDED","start":"2026-11-17T01:21:00Z"}]"#),
        "{json}"
    );
    common::assert_same_information(&text, &json);
}

#[tokio::test]
async fn smart_notes_403_is_tolerated_like_other_artifacts() {
    for (status, message) in [
        (403, "no notes access"),
        (404, "Requested entity was not found."),
    ] {
        let harness = Harness::start().await;
        mount_meeting(&harness).await;
        harness
            .mount_records(
                records_filter(),
                json!([{"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z"}]),
            )
            .await;
        for kind in ["participants", "transcripts"] {
            harness.mount_collection("REC1", kind, json!([])).await;
        }
        harness
            .mount_collection(
                "REC1",
                "recordings",
                json!([{"state": "ENDED", "driveDestination": {"file": "F1"}}]),
            )
            .await;
        harness
            .mount_status(
                "/v2/conferenceRecords/REC1/smartNotes",
                status,
                json!({"error": {"code": status, "message": message}}),
            )
            .await;
        let output = get_output(&harness, "evt1").await.unwrap();
        assert!(
            output.contains(
                "conference: id=REC1 start=2026-11-17T01:00:00Z\nrecording: state=ENDED drive_file=F1\n"
            ),
            "{output}"
        );
        assert!(!output.contains("smart_note"), "{output}");
        assert!(
            output.ends_with(&format!("meet_api: unavailable ({status} {message})\n")),
            "{output}"
        );
    }
}

#[tokio::test]
async fn attachments_appear_in_meet_get_with_the_new_mask() {
    let harness = Harness::start().await;
    let mut value = meet_event("evt1", CODE, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z");
    value["attachments"] = json!([
        {"fileUrl": "https://docs.google.com/document/d/1AbCdEf/edit", "title": "Notes by Gemini", "mimeType": "application/vnd.google-apps.document", "fileId": "1AbCdEf", "iconLink": "https://x.example/i.png"},
        {"title": "Agenda", "mimeType": "application/pdf"}
    ]);
    harness
        .mount_json(&format!("{EVENTS_PATH}/evt1"), value)
        .await;
    harness
        .mount_json(
            &format!("/v2/spaces/{CODE}"),
            json!({"config": {"moderation": "OFF"}}),
        )
        .await;
    harness.mount_records(records_filter(), json!([])).await;
    let text = get_output(&harness, "evt1").await.unwrap();
    assert!(
        text.ends_with(
            "moderation: OFF\n\
             attachment: title=\"Notes by Gemini\" mime_type=application/vnd.google-apps.document file_id=1AbCdEf\n\
             attachment: title=Agenda mime_type=application/pdf\n\
             conference: none\n"
        ),
        "{text}"
    );
    assert!(!text.contains("fileUrl") && !text.contains("x.example"));
    let mut out = Vec::new();
    meet::get(&harness.api, "evt1", true, &mut out)
        .await
        .unwrap();
    let json = common::text(out);
    assert!(
        json.contains(r#""moderation":"OFF","attachments":[{"title":"Notes by Gemini","mime_type":"application/vnd.google-apps.document","file_id":"1AbCdEf"},{"title":"Agenda","mime_type":"application/pdf"}],"conferences":[]"#),
        "{json}"
    );
    common::assert_same_information(&text, &json);
    assert_eq!(
        harness.requests(&format!("{EVENTS_PATH}/evt1")).await[0]["fields"],
        EVENT_FIELDS
    );
}

#[tokio::test]
async fn no_smart_notes_means_no_key() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness
        .mount_records(
            records_filter(),
            json!([{"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z"}]),
        )
        .await;
    mount_other_artifacts(&harness, "REC1").await;
    harness
        .mount_collection("REC1", "smartNotes", json!([]))
        .await;
    let text = get_output(&harness, "evt1").await.unwrap();
    assert!(
        text.ends_with("moderation: OFF\nconference: id=REC1 start=2026-11-17T01:00:00Z\n"),
        "{text}"
    );
    let mut out = Vec::new();
    meet::get(&harness.api, "evt1", true, &mut out)
        .await
        .unwrap();
    let json = common::text(out);
    assert!(!json.contains("smart_note"), "{json}");
    assert!(!json.contains("attachments"), "{json}");
    assert_eq!(
        harness
            .requests("/v2/conferenceRecords/REC1/smartNotes")
            .await
            .len(),
        2
    );
}

#[tokio::test]
async fn get_without_conference_records_prints_the_event_and_space_only() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness.mount_records(records_filter(), json!([])).await;
    let output = get_output(&harness, "evt1").await.unwrap();
    assert!(
        output.ends_with("moderation: OFF\nconference: none\n"),
        "{output}"
    );
}

#[tokio::test]
async fn get_for_an_all_day_event_filters_by_meeting_code_only() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/day1"),
            json!({"id": "day1", "summary": "Offsite", "start": {"date": "2026-11-20"}, "end": {"date": "2026-11-21"},
                   "conferenceData": {"conferenceId": CODE, "conferenceSolution": {"key": {"type": "hangoutsMeet"}}}}),
        )
        .await;
    harness
        .mount_json(&format!("/v2/spaces/{CODE}"), json!({}))
        .await;
    harness
        .mount_records("space.meeting_code = \"abc-defg-hij\"", json!([]))
        .await;
    let output = get_output(&harness, "day1").await.unwrap();
    assert!(
        output.contains("start: 2026-11-20\nend: 2026-11-21\n"),
        "{output}"
    );
    assert_eq!(
        harness.requests("/v2/conferenceRecords").await[0]["filter"],
        "space.meeting_code = \"abc-defg-hij\""
    );
}

#[tokio::test]
async fn get_rejects_events_without_a_meet_conference() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/plain"),
            event("plain", "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
        )
        .await;
    let error = get_output(&harness, "plain").await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "event plain has no google meet conference"
    );
    assert!(
        harness
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().starts_with("/v2/"))
    );
}

#[tokio::test]
async fn get_reports_an_unknown_event() {
    let harness = Harness::start().await;
    harness
        .mount_status(
            &format!("{EVENTS_PATH}/nope"),
            404,
            json!({"error": {"code": 404, "message": "Not Found"}}),
        )
        .await;
    let error = get_output(&harness, "nope").await.unwrap_err();
    assert_eq!(gdcli::message(&error), "google api error 404: Not Found");
}

#[tokio::test]
async fn get_tolerates_meet_403_and_404_with_a_single_line() {
    for (status, message) in [
        (403, "The caller does not have permission"),
        (404, "Requested entity was not found."),
    ] {
        let harness = Harness::start().await;
        harness
            .mount_json(
                &format!("{EVENTS_PATH}/evt1"),
                meet_event("evt1", CODE, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
            )
            .await;
        harness
            .mount_status(
                &format!("/v2/spaces/{CODE}"),
                status,
                json!({"error": {"code": status, "message": message}}),
            )
            .await;
        harness.mount_records(records_filter(), json!([])).await;
        assert_eq!(
            get_output(&harness, "evt1").await.unwrap(),
            format!(
                "id: evt1\n\
                 summary: Event evt1\n\
                 start: 2026-11-17T01:00:00Z\n\
                 end: 2026-11-17T02:00:00Z\n\
                 meeting_code: abc-defg-hij\n\
                 meeting_uri: https://meet.google.com/abc-defg-hij\n\
                 conference: none\n\
                 meet_api: unavailable ({status} {message})\n"
            )
        );
    }
}

#[tokio::test]
async fn get_tolerates_a_forbidden_conference_list_too() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness
        .mount_status(
            "/v2/conferenceRecords",
            403,
            json!({"error": {"code": 403, "message": "denied"}}),
        )
        .await;
    let output = get_output(&harness, "evt1").await.unwrap();
    assert!(
        output.ends_with("meet_api: unavailable (403 denied)\n"),
        "{output}"
    );
    assert!(output.contains("access_type: TRUSTED\n"));
    assert!(!output.contains("conference:"));
}

#[tokio::test]
async fn get_fails_on_other_meet_errors_without_printing_anything() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness
        .mount_status(
            "/v2/conferenceRecords",
            500,
            json!({"error": {"code": 500, "message": "backend error"}}),
        )
        .await;
    let error = get_output(&harness, "evt1").await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 500: backend error"
    );
}

#[tokio::test]
async fn get_prints_what_arrived_when_one_artifact_call_is_forbidden() {
    let harness = Harness::start().await;
    mount_meeting(&harness).await;
    harness
        .mount_records(
            records_filter(),
            json!([{"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z"}]),
        )
        .await;
    harness
        .mount_status(
            "/v2/conferenceRecords/REC1/participants",
            403,
            json!({"error": {"code": 403, "message": "no participant access"}}),
        )
        .await;
    harness
        .mount_json(
            "/v2/conferenceRecords/REC1/recordings",
            json!({"recordings": [{"state": "ENDED", "driveDestination": {"file": "F1"}}]}),
        )
        .await;
    harness
        .mount_json("/v2/conferenceRecords/REC1/transcripts", json!({}))
        .await;
    harness
        .mount_json("/v2/conferenceRecords/REC1/smartNotes", json!({}))
        .await;
    let output = get_output(&harness, "evt1").await.unwrap();
    assert!(
        output.contains(
            "conference: id=REC1 start=2026-11-17T01:00:00Z\nrecording: state=ENDED drive_file=F1\n"
        ),
        "{output}"
    );
    assert!(
        output.ends_with("meet_api: unavailable (403 no participant access)\n"),
        "{output}"
    );
}

#[tokio::test]
async fn list_counts_and_json_follow_the_same_rows() {
    let harness = Harness::start().await;
    let events: Vec<Value> = (1..=4)
        .map(|day| at_day(&format!("p{day}"), "aaa-bbbb-ccc", 10 + day, 1))
        .chain((1..=4).map(|day| at_day(&format!("u{day}"), "ddd-eeee-fff", 18 + day, 1)))
        .collect();
    harness.mount_calendar(events).await;
    let args = gdcli::cli::ListArgs {
        past: 2,
        upcoming: 3,
        json: true,
        ..common::args()
    };
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &args, &mut out)
        .await
        .unwrap();
    let rows: Vec<Value> = serde_json::from_str(&text(out)).unwrap();
    let found: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap(),
                row["when"].as_str().unwrap(),
                row["meeting_code"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("p3", "past", "aaa-bbbb-ccc"),
            ("p4", "past", "aaa-bbbb-ccc"),
            ("u1", "upcoming", "ddd-eeee-fff"),
            ("u2", "upcoming", "ddd-eeee-fff"),
            ("u3", "upcoming", "ddd-eeee-fff"),
        ]
    );
    assert_eq!(
        rows[0].as_object().unwrap().keys().collect::<Vec<_>>(),
        HEADER.split('\t').collect::<Vec<_>>()
    );
    assert_eq!(list_json_empty().await, "[]\n");
}

async fn list_json_empty() -> String {
    let harness = Harness::start_with(Vec::new()).await;
    let args = gdcli::cli::ListArgs {
        past: 5,
        upcoming: 5,
        json: true,
        ..common::args()
    };
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &args, &mut out)
        .await
        .unwrap();
    text(out)
}

#[tokio::test]
async fn meet_upcoming_pages_use_at_least_25_and_at_most_250_events() {
    for (requested, expected) in [
        (1, "25"),
        (3, "25"),
        (25, "25"),
        (100, "100"),
        (400, "250"),
        (1000, "250"),
    ] {
        let harness = Harness::start_with(vec![at_day("m1", CODE, 20, 1)]).await;
        let args = gdcli::cli::ListArgs {
            past: 0,
            upcoming: requested,
            json: false,
            ..common::args()
        };
        meet::list(&harness.api, now(), &args, &mut Vec::new())
            .await
            .unwrap();
        let sizes: Vec<String> = harness
            .requests(EVENTS_PATH)
            .await
            .into_iter()
            .map(|query| query["maxResults"].clone())
            .collect();
        assert_eq!(sizes, [expected], "{requested}");
    }
}
