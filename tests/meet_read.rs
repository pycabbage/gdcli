mod common;

use clap::Parser;
use common::{
    BEARER, CODE, EVENT_FIELDS, EVENTS_PATH, Harness, RECORDS_FIELDS, event, meet_event,
    records_filter, text,
};
use gdcli::cli::{Cli, Command, MeetCommand, ReadArgs};
use gdcli::{docs, meet};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const NOTE_FIELDS: &str = "smartNotes(state,startTime,endTime,docsDestination/document)";
const NOTES_MISSING: &str = "no gemini notes document for event evt1";
const NO_RECORD: &str = "no conference record for event evt1; Meet keeps a record only from the start of a meeting until 30 days after it (see the attachment lines of calendar get)";

async fn event_only() -> Harness {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/evt1"),
            meet_event("evt1", CODE, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
        )
        .await;
    harness
}

async fn start(records: &[&str]) -> Harness {
    let harness = event_only().await;
    let records: Vec<Value> = records
        .iter()
        .map(|name| json!({"name": format!("conferenceRecords/{name}"), "startTime": "2026-11-17T01:00:00Z"}))
        .collect();
    harness
        .mount_records(records_filter(), json!(records))
        .await;
    harness
}

fn item(state: &str, document: Option<&str>, start: &str, end: &str) -> Value {
    let mut value = json!({"state": state, "startTime": start, "endTime": end});
    if let Some(document) = document {
        value["docsDestination"] = json!({"document": document});
    }
    value
}

fn ready(document: &str, start: &str, end: &str) -> Value {
    item("FILE_GENERATED", Some(document), start, end)
}

fn read_args(transcript: bool, tab: Option<&str>) -> ReadArgs {
    let name = if transcript { "transcript" } else { "notes" };
    let mut args = vec!["gdcli", "meet", name, "evt1"];
    args.extend(tab.into_iter().flat_map(|tab| ["--tab", tab]));
    match Cli::try_parse_from(args).unwrap().command {
        Command::Meet(MeetCommand::Notes(args) | MeetCommand::Transcript(args)) => args,
        other => panic!("{other:?}"),
    }
}

async fn run(
    harness: &Harness,
    transcript: bool,
    tab: Option<&str>,
) -> (Result<(), String>, String) {
    let args = read_args(transcript, tab);
    let mut out = Vec::new();
    let result = if transcript {
        meet::transcript(&harness.api, &args, &mut out).await
    } else {
        meet::notes(&harness.api, &args, &mut out).await
    };
    (result.map_err(|error| gdcli::message(&error)), text(out))
}

async fn failure(harness: &Harness, transcript: bool, tab: Option<&str>) -> String {
    let (result, output) = run(harness, transcript, tab).await;
    assert_eq!(output, "", "nothing may be printed on failure");
    result.unwrap_err()
}

async fn mount_tabs(harness: &Harness, id: &str) {
    harness
        .mount_json(
            &format!("/v1/documents/{id}"),
            json!({"tabs": [
                {"tabProperties": {"tabId": "t.0", "title": "Quick notes"}},
                {"tabProperties": {"tabId": "t.1", "title": "Full notes", "index": 1}}
            ]}),
        )
        .await;
}

async fn mount_tab_export(harness: &Harness, id: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/document/d/{id}/export")))
        .and(query_param("format", "md"))
        .and(query_param("tab", "t.1"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "location",
            format!("{}/blob/{id}", harness.server.uri()).as_str(),
        ))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/blob/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!("full notes of {id}"),
            "text/x-markdown; charset=utf-8",
        ))
        .mount(&harness.server)
        .await;
}

async fn paths(harness: &Harness) -> Vec<String> {
    harness
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect()
}

#[tokio::test]
async fn one_document_is_the_exported_markdown_unchanged() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T01:20:00Z")]),
        )
        .await;
    let markdown = "Quick notes\n\n- summary\n\nFull notes\n\n- details";
    harness.mount_doc("N1", markdown).await;
    let (result, output) = run(&harness, false, None).await;
    result.unwrap();
    assert_eq!(output, markdown);
    let mut direct = Vec::new();
    docs::get(&harness.api, "N1", None, &mut direct)
        .await
        .unwrap();
    assert_eq!(text(direct), output);
}

#[tokio::test]
async fn several_documents_are_oldest_first_with_document_lines() {
    let harness = start(&["REC2", "REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T01:10:00Z")]),
        )
        .await;
    harness
        .mount_collection(
            "REC2",
            "smartNotes",
            json!([
                ready("N2", "2026-11-17T01:31:00Z", "2026-11-17T01:40:00Z"),
                ready("N3", "2026-11-17T01:41:00Z", "2026-11-17T01:50:00Z")
            ]),
        )
        .await;
    for (id, markdown) in [("N1", "one"), ("N2", "two"), ("N3", "three")] {
        harness.mount_doc(id, markdown).await;
    }
    let (result, output) = run(&harness, false, None).await;
    result.unwrap();
    assert_eq!(
        output,
        "document: id=N1 start=2026-11-17T01:00:09Z end=2026-11-17T01:10:00Z\none\n\n\
         document: id=N2 start=2026-11-17T01:31:00Z end=2026-11-17T01:40:00Z\ntwo\n\n\
         document: id=N3 start=2026-11-17T01:41:00Z end=2026-11-17T01:50:00Z\nthree\n\n"
    );
}

#[tokio::test]
async fn requests_are_exact() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T01:20:00Z")]),
        )
        .await;
    harness.mount_doc("N1", "notes").await;
    run(&harness, false, None).await.0.unwrap();
    assert_eq!(
        paths(&harness).await,
        [
            format!("{EVENTS_PATH}/evt1"),
            "/v2/conferenceRecords".to_owned(),
            "/v2/conferenceRecords/REC1/smartNotes".to_owned(),
            "/drive/v3/files/N1/export".to_owned(),
        ]
    );
    for request in harness.server.received_requests().await.unwrap() {
        assert_eq!(request.method.as_str(), "GET");
        assert_eq!(
            request
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            BEARER
        );
    }
    let events = harness.requests(&format!("{EVENTS_PATH}/evt1")).await;
    assert_eq!(events[0]["fields"], EVENT_FIELDS);
    let records = &harness.requests("/v2/conferenceRecords").await[0];
    assert_eq!(records["filter"], records_filter());
    assert_eq!(records["pageSize"], "5");
    assert_eq!(records["fields"], RECORDS_FIELDS);
    let notes = &harness
        .requests("/v2/conferenceRecords/REC1/smartNotes")
        .await[0];
    assert_eq!(notes["pageSize"], "100");
    assert_eq!(notes["fields"], NOTE_FIELDS);
    let export = &harness.requests("/drive/v3/files/N1/export").await[0];
    assert_eq!(export["mimeType"], "text/markdown");
}

#[tokio::test]
async fn transcript_uses_the_transcripts_collection() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T01:20:00Z")]),
        )
        .await;
    harness
        .mount_collection(
            "REC1",
            "transcripts",
            json!([ready("T1", "2026-11-17T01:00:05Z", "2026-11-17T01:20:00Z")]),
        )
        .await;
    harness.mount_doc("N1", "the notes").await;
    harness.mount_doc("T1", "the transcript").await;
    assert_eq!(run(&harness, true, None).await.1, "the transcript");
    let route = |kind: &str| format!("/v2/conferenceRecords/REC1/{kind}");
    assert_eq!(harness.requests(&route("transcripts")).await.len(), 1);
    assert_eq!(harness.requests(&route("smartNotes")).await.len(), 0);
    assert_eq!(run(&harness, false, None).await.1, "the notes");
    assert_eq!(harness.requests(&route("transcripts")).await.len(), 1);
    assert_eq!(harness.requests(&route("smartNotes")).await.len(), 1);
    assert_eq!(harness.requests("/drive/v3/files/T1/export").await.len(), 1);
    assert_eq!(harness.requests("/drive/v3/files/N1/export").await.len(), 1);
}

#[tokio::test]
async fn tab_exports_one_tab_of_each_document() {
    let harness = start(&["REC2", "REC1"]).await;
    for (record, document, start) in [("REC1", "N1", "01:00:09"), ("REC2", "N2", "01:31:00")] {
        harness
            .mount_collection(
                record,
                "smartNotes",
                json!([ready(
                    document,
                    &format!("2026-11-17T{start}Z"),
                    "2026-11-17T02:00:00Z"
                )]),
            )
            .await;
        mount_tabs(&harness, document).await;
        mount_tab_export(&harness, document).await;
    }
    let (result, output) = run(&harness, false, Some("Full notes")).await;
    result.unwrap();
    assert_eq!(
        output,
        "document: id=N1 start=2026-11-17T01:00:09Z end=2026-11-17T02:00:00Z\nfull notes of N1\n\n\
         document: id=N2 start=2026-11-17T01:31:00Z end=2026-11-17T02:00:00Z\nfull notes of N2\n\n"
    );
    for id in ["N1", "N2"] {
        assert_eq!(
            harness.requests(&format!("/v1/documents/{id}")).await.len(),
            1
        );
        assert_eq!(
            harness
                .requests(&format!("/document/d/{id}/export"))
                .await
                .len(),
            1
        );
        assert_eq!(
            harness
                .requests(&format!("/drive/v3/files/{id}/export"))
                .await
                .len(),
            0
        );
    }
    let single = start(&["REC1"]).await;
    single
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T02:00:00Z")]),
        )
        .await;
    mount_tabs(&single, "N1").await;
    mount_tab_export(&single, "N1").await;
    let (result, output) = run(&single, false, Some("t.1")).await;
    result.unwrap();
    let mut direct = Vec::new();
    docs::get(&single.api, "N1", Some("t.1"), &mut direct)
        .await
        .unwrap();
    assert_eq!(text(direct), output);
    assert_eq!(output, "full notes of N1");
    single
        .mount_collection(
            "REC1",
            "transcripts",
            json!([ready("T1", "2026-11-17T01:00:05Z", "2026-11-17T02:00:00Z")]),
        )
        .await;
    mount_tabs(&single, "T1").await;
    mount_tab_export(&single, "T1").await;
    let (result, output) = run(&single, true, Some("Full notes")).await;
    result.unwrap();
    assert_eq!(output, "full notes of T1");
    assert_eq!(single.requests("/drive/v3/files/T1/export").await.len(), 0);
}

#[tokio::test]
async fn no_conference_record() {
    let harness = start(&[]).await;
    for transcript in [false, true] {
        assert_eq!(failure(&harness, transcript, None).await, NO_RECORD);
    }
    assert!(
        paths(&harness)
            .await
            .iter()
            .all(|route| route == "/v2/conferenceRecords" || route.starts_with("/calendar/"))
    );
}

#[tokio::test]
async fn no_notes_or_transcript() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_json("/v2/conferenceRecords/REC1/smartNotes", json!({}))
        .await;
    harness
        .mount_collection("REC1", "transcripts", json!([]))
        .await;
    assert_eq!(failure(&harness, false, None).await, NOTES_MISSING);
    assert_eq!(
        failure(&harness, true, None).await,
        "no transcript for event evt1"
    );
}

#[tokio::test]
async fn not_ready_state_is_named() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([item(
                "ENDED",
                Some("N1"),
                "2026-11-17T01:00:09Z",
                "2026-11-17T01:20:00Z"
            )]),
        )
        .await;
    harness
        .mount_collection(
            "REC1",
            "transcripts",
            json!([item(
                "STARTED",
                None,
                "2026-11-17T01:00:05Z",
                "2026-11-17T01:20:00Z"
            )]),
        )
        .await;
    assert_eq!(
        failure(&harness, false, None).await,
        "the gemini notes document of event evt1 is not ready (state ENDED)"
    );
    assert_eq!(
        failure(&harness, true, None).await,
        "the transcript of event evt1 is not ready (state STARTED)"
    );
    assert_eq!(harness.requests("/drive/v3/files/N1/export").await.len(), 0);
}

#[tokio::test]
async fn event_without_a_meet_conference() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/evt1"),
            event("evt1", "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
        )
        .await;
    for transcript in [false, true] {
        assert_eq!(
            failure(&harness, transcript, None).await,
            "event evt1 has no google meet conference"
        );
    }
    assert!(
        paths(&harness)
            .await
            .iter()
            .all(|route| !route.starts_with("/v2/"))
    );
}

#[tokio::test]
async fn an_unreadable_document_fails_before_any_output() {
    let harness = start(&["REC2", "REC1"]).await;
    for (record, kind, document) in [
        ("REC1", "smartNotes", "N1"),
        ("REC2", "smartNotes", "N2"),
        ("REC1", "transcripts", "T1"),
        ("REC2", "transcripts", "T2"),
    ] {
        harness
            .mount_collection(
                record,
                kind,
                json!([ready(
                    document,
                    "2026-11-17T01:00:09Z",
                    "2026-11-17T02:00:00Z"
                )]),
            )
            .await;
    }
    harness.mount_doc("N1", "first").await;
    harness.mount_doc("T1", "first").await;
    harness
        .mount_status(
            "/drive/v3/files/N2/export",
            403,
            json!({"error": {"code": 403, "message": "The user does not have sufficient permissions for this file."}}),
        )
        .await;
    harness
        .mount_status(
            "/drive/v3/files/T2/export",
            404,
            json!({"error": {"code": 404, "message": "File not found: T2."}}),
        )
        .await;
    assert_eq!(
        failure(&harness, false, None).await,
        "cannot read gemini notes document N2: google api error 403: The user does not have sufficient permissions for this file."
    );
    assert_eq!(
        failure(&harness, true, None).await,
        "cannot read transcript T2: google api error 404: File not found: T2."
    );
    for id in ["N1", "N2", "T1", "T2"] {
        assert_eq!(
            harness
                .requests(&format!("/drive/v3/files/{id}/export"))
                .await
                .len(),
            1,
            "{id}"
        );
    }
    let single = start(&["REC1"]).await;
    single
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([ready("N1", "2026-11-17T01:00:09Z", "2026-11-17T02:00:00Z")]),
        )
        .await;
    mount_tabs(&single, "N1").await;
    assert_eq!(
        failure(&single, false, Some("nope")).await,
        "cannot read gemini notes document N1: tab \"nope\" not found; available: \"Quick notes\" (t.0), \"Full notes\" (t.1)"
    );
}

#[tokio::test]
async fn meet_api_403_on_the_record_list_is_an_error() {
    let harness = event_only().await;
    harness
        .mount_status(
            "/v2/conferenceRecords",
            403,
            json!({"error": {"code": 403, "message": "denied"}}),
        )
        .await;
    for transcript in [false, true] {
        assert_eq!(
            failure(&harness, transcript, None).await,
            "google api error 403: denied"
        );
    }
    let harness = start(&["REC1"]).await;
    harness
        .mount_status(
            "/v2/conferenceRecords/REC1/smartNotes",
            403,
            json!({"error": {"code": 403, "message": "no access"}}),
        )
        .await;
    assert_eq!(
        failure(&harness, false, None).await,
        "google api error 403: no access"
    );
}

#[tokio::test]
async fn only_file_generated_documents_with_an_id_count() {
    let harness = start(&["REC1"]).await;
    harness
        .mount_collection(
            "REC1",
            "smartNotes",
            json!([
                item(
                    "ENDED",
                    Some("N0"),
                    "2026-11-17T01:00:00Z",
                    "2026-11-17T01:01:00Z"
                ),
                item(
                    "FILE_GENERATED",
                    None,
                    "2026-11-17T01:02:00Z",
                    "2026-11-17T01:03:00Z"
                ),
                ready("N3", "2026-11-17T01:04:00Z", "2026-11-17T01:05:00Z"),
                item(
                    "STARTED",
                    None,
                    "2026-11-17T01:06:00Z",
                    "2026-11-17T01:07:00Z"
                )
            ]),
        )
        .await;
    harness.mount_doc("N3", "only this").await;
    let (result, output) = run(&harness, false, None).await;
    result.unwrap();
    assert_eq!(output, "only this");
    assert_eq!(harness.requests("/drive/v3/files/N0/export").await.len(), 0);
    harness
        .mount_collection(
            "REC1",
            "transcripts",
            json!([
                item(
                    "STARTED",
                    None,
                    "2026-11-17T01:00:00Z",
                    "2026-11-17T01:01:00Z"
                ),
                item(
                    "ENDED",
                    None,
                    "2026-11-17T01:02:00Z",
                    "2026-11-17T01:03:00Z"
                )
            ]),
        )
        .await;
    assert_eq!(
        failure(&harness, true, None).await,
        "the transcript of event evt1 is not ready (state ENDED)"
    );
    let other = start(&["REC1"]).await;
    other
        .mount_collection(
            "REC1",
            "transcripts",
            json!([item(
                "FILE_GENERATED",
                None,
                "2026-11-17T01:00:00Z",
                "2026-11-17T01:01:00Z"
            )]),
        )
        .await;
    assert_eq!(
        failure(&other, true, None).await,
        "the transcript of event evt1 is not ready (state FILE_GENERATED)"
    );
}
