mod common;

const RECORDS_FIELDS: &str = "conferenceRecords(name,startTime,endTime,space)";
const SPACE_FIELDS: &str =
    "name,meetingUri,meetingCode,config(accessType,entryPointAccess,moderation),activeConference";
const UPCOMING_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";

use common::{Harness, event, meet_event, now, text};
use gdcli::meet;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn list_merges_past_records_and_upcoming_meet_events() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            "/v2/conferenceRecords",
            json!({"conferenceRecords": [
                {"name": "conferenceRecords/r3", "startTime": "2026-11-17T01:00:00Z", "endTime": "2026-11-17T02:00:00Z", "space": "spaces/s3"},
                {"name": "conferenceRecords/r2", "startTime": "2026-11-16T01:00:00Z", "endTime": "2026-11-16T02:00:00Z", "space": "spaces/s2"},
                {"name": "conferenceRecords/r1", "startTime": "2026-11-15T01:00:00Z", "space": "spaces/s1"}
            ]}),
        )
        .await;
    harness
        .mount_calendar(vec![
            event("plain", "2026-11-19T01:00:00Z", "2026-11-19T02:00:00Z"),
            meet_event(
                "m1",
                "aaa-bbbb-ccc",
                "2026-11-20T01:00:00Z",
                "2026-11-20T02:00:00Z",
            ),
            {
                let mut other = meet_event(
                    "other",
                    "zzz",
                    "2026-11-20T03:00:00Z",
                    "2026-11-20T04:00:00Z",
                );
                other["conferenceData"]["conferenceSolution"]["key"]["type"] = json!("addOn");
                other
            },
            common::cancelled("dead", "2026-11-21T01:00:00Z", "2026-11-21T02:00:00Z"),
            meet_event(
                "m2",
                "ddd-eeee-fff",
                "2026-11-22T01:00:00Z",
                "2026-11-22T02:00:00Z",
            ),
        ])
        .await;
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &mut out).await.unwrap();
    assert_eq!(
        text(out),
        "past\tr1\t2026-11-15T01:00:00Z\t\tspaces/s1\n\
         past\tr2\t2026-11-16T01:00:00Z\t2026-11-16T02:00:00Z\tspaces/s2\n\
         past\tr3\t2026-11-17T01:00:00Z\t2026-11-17T02:00:00Z\tspaces/s3\n\
         upcoming\taaa-bbbb-ccc\t2026-11-20T01:00:00Z\t2026-11-20T02:00:00Z\tEvent m1\n\
         upcoming\tddd-eeee-fff\t2026-11-22T01:00:00Z\t2026-11-22T02:00:00Z\tEvent m2\n"
    );
    let records = harness.requests("/v2/conferenceRecords").await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["pageSize"], "5");
    assert_eq!(records[0]["fields"], RECORDS_FIELDS);
    assert!(!records[0].contains_key("filter"));
    let calendar = harness.requests(common::EVENTS_PATH).await;
    assert_eq!(calendar.len(), 1);
    assert_eq!(calendar[0]["fields"], UPCOMING_FIELDS);
    assert_eq!(calendar[0]["maxResults"], "25");
    assert_eq!(calendar[0]["singleEvents"], "true");
    assert_eq!(calendar[0]["orderBy"], "startTime");
}

#[tokio::test]
async fn list_pages_calendar_until_five_meet_events_are_found() {
    let harness = Harness::start().await;
    harness.mount_json("/v2/conferenceRecords", json!({})).await;
    let mut events: Vec<_> = Vec::new();
    events.extend((0..30).map(|index| {
        event(
            &format!("n{index}"),
            &format!("2027-01-01T{:02}:{:02}:00Z", index / 6, (index % 6) * 10),
            &format!("2027-01-01T{:02}:{:02}:30Z", index / 6, (index % 6) * 10),
        )
    }));
    events.extend((0..5).map(|index| {
        meet_event(
            &format!("m{index}"),
            &format!("abc-defg-hi{index}"),
            &format!("2027-02-01T0{index}:00:00Z"),
            &format!("2027-02-01T0{index}:30:00Z"),
        )
    }));
    harness.mount_calendar(events).await;
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &mut out).await.unwrap();
    let lines = text(out);
    assert_eq!(lines.lines().count(), 5);
    assert!(
        lines
            .lines()
            .all(|line| line.starts_with("upcoming\tabc-defg-hi"))
    );
    let upcoming = harness
        .requests(common::EVENTS_PATH)
        .await
        .into_iter()
        .filter(|query| !query.contains_key("timeMax"))
        .count();
    assert_eq!(upcoming, 2);
}

#[tokio::test]
async fn get_by_meeting_code_shows_space_and_recent_conferences() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            "/v2/spaces/abc-defg-hij",
            json!({
                "name": "spaces/SP1",
                "meetingUri": "https://meet.google.com/abc-defg-hij",
                "meetingCode": "abc-defg-hij",
                "config": {"accessType": "TRUSTED", "entryPointAccess": "ALL"},
                "activeConference": {"conferenceRecord": "conferenceRecords/live"}
            }),
        )
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/conferenceRecords"))
        .and(header("authorization", common::BEARER))
        .and(query_param("filter", "space.meeting_code = \"abc-defg-hij\""))
        .and(query_param("pageSize", "5"))
        .and(query_param("fields", RECORDS_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"conferenceRecords": [
            {"name": "conferenceRecords/new", "startTime": "2026-11-17T01:00:00Z"},
            {"name": "conferenceRecords/old", "startTime": "2026-11-10T01:00:00Z", "endTime": "2026-11-10T02:00:00Z"}
        ]})))
        .mount(&harness.server)
        .await;
    let mut out = Vec::new();
    meet::get(&harness.api, "abc-defg-hij", &mut out)
        .await
        .unwrap();
    assert_eq!(
        text(out),
        "id: abc-defg-hij\n\
         space: spaces/SP1\n\
         meeting_code: abc-defg-hij\n\
         meeting_uri: https://meet.google.com/abc-defg-hij\n\
         access_type: TRUSTED\n\
         entry_point_access: ALL\n\
         active_conference: live\n\
         conference: old 2026-11-10T01:00:00Z 2026-11-10T02:00:00Z\n\
         conference: new 2026-11-17T01:00:00Z\n"
    );
    let spaces = harness.requests("/v2/spaces/abc-defg-hij").await;
    assert_eq!(spaces.len(), 1);
    assert_eq!(spaces[0]["fields"], SPACE_FIELDS);
}

#[tokio::test]
async fn get_by_meeting_code_is_case_insensitive() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            "/v2/spaces/abc-defg-hij",
            json!({"name": "spaces/SP1", "meetingCode": "abc-defg-hij"}),
        )
        .await;
    harness.mount_json("/v2/conferenceRecords", json!({})).await;
    let mut out = Vec::new();
    meet::get(&harness.api, "ABC-DEFG-HIJ", &mut out)
        .await
        .unwrap();
    assert!(text(out).starts_with("id: abc-defg-hij\n"));
}

async fn mount_record(harness: &Harness) {
    harness
        .mount_json(
            "/v2/conferenceRecords/REC1",
            json!({"name": "conferenceRecords/REC1", "startTime": "2026-11-17T01:00:00Z", "endTime": "2026-11-17T02:00:00Z", "space": "spaces/SP1"}),
        )
        .await;
    harness
        .mount_json(
            "/v2/spaces/SP1",
            json!({"name": "spaces/SP1", "meetingCode": "abc-defg-hij", "meetingUri": "https://meet.google.com/abc-defg-hij"}),
        )
        .await;
    harness
        .mount_json(
            "/v2/conferenceRecords/REC1/participants",
            json!({"participants": [
                {"signedinUser": {"user": "users/1", "displayName": "Alice"}, "earliestStartTime": "2026-11-17T01:00:00Z", "latestEndTime": "2026-11-17T02:00:00Z"},
                {"anonymousUser": {"displayName": "Guest"}, "earliestStartTime": "2026-11-17T01:05:00Z"},
                {"phoneUser": {"displayName": "+81 90"}}
            ]}),
        )
        .await;
    harness
        .mount_json(
            "/v2/conferenceRecords/REC1/recordings",
            json!({"recordings": [{"state": "FILE_GENERATED", "startTime": "2026-11-17T01:00:00Z", "endTime": "2026-11-17T02:00:00Z", "driveDestination": {"file": "FILE1"}}]}),
        )
        .await;
    harness
        .mount_json(
            "/v2/conferenceRecords/REC1/transcripts",
            json!({"transcripts": [{"state": "ENDED", "docsDestination": {"document": "DOC1"}}]}),
        )
        .await;
}

#[tokio::test]
async fn get_by_record_id_with_and_without_prefix() {
    let harness = Harness::start().await;
    mount_record(&harness).await;
    let expected = "id: REC1\n\
         start: 2026-11-17T01:00:00Z\n\
         end: 2026-11-17T02:00:00Z\n\
         space: spaces/SP1\n\
         meeting_code: abc-defg-hij\n\
         meeting_uri: https://meet.google.com/abc-defg-hij\n\
         participant: Alice (user) 2026-11-17T01:00:00Z 2026-11-17T02:00:00Z\n\
         participant: Guest (anonymous) 2026-11-17T01:05:00Z\n\
         participant: +81 90 (phone)\n\
         recording: FILE_GENERATED 2026-11-17T01:00:00Z 2026-11-17T02:00:00Z drive:FILE1\n\
         transcript: ENDED doc:DOC1\n";
    for id in ["REC1", "conferenceRecords/REC1"] {
        let mut out = Vec::new();
        meet::get(&harness.api, id, &mut out).await.unwrap();
        assert_eq!(text(out), expected);
    }
    for (route, fields, page_size) in [
        (
            "/v2/conferenceRecords/REC1",
            "name,startTime,endTime,space",
            None,
        ),
        ("/v2/spaces/SP1", SPACE_FIELDS, None),
        (
            "/v2/conferenceRecords/REC1/participants",
            "participants(earliestStartTime,latestEndTime,signedinUser,anonymousUser,phoneUser)",
            Some("250"),
        ),
        (
            "/v2/conferenceRecords/REC1/recordings",
            "recordings(state,startTime,endTime,driveDestination/file)",
            Some("100"),
        ),
        (
            "/v2/conferenceRecords/REC1/transcripts",
            "transcripts(state,startTime,endTime,docsDestination/document)",
            Some("100"),
        ),
    ] {
        let requests = harness.requests(route).await;
        assert_eq!(requests.len(), 2, "{route}");
        for query in requests {
            assert_eq!(query["fields"], fields, "{route}");
            assert_eq!(
                query.get("pageSize").map(String::as_str),
                page_size,
                "{route}"
            );
        }
    }
}

#[tokio::test]
async fn get_unknown_record_fails_with_api_message() {
    let harness = Harness::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_json(
            json!({"error": {"code": 404, "message": "Requested entity was not found."}}),
        ))
        .mount(&harness.server)
        .await;
    let mut out = Vec::new();
    let error = meet::get(&harness.api, "nope", &mut out).await.unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 404: Requested entity was not found."
    );
}
