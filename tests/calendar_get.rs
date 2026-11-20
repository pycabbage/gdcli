mod common;

use common::{EVENTS_PATH, Harness, assert_same_information, text};
use gdcli::calendar;
use serde_json::{Value, json};

fn event() -> Value {
    json!({
        "id": "abc123", "status": "confirmed", "summary": "Planning",
        "description": "First line\r\nSecond line", "location": "Room 1",
        "htmlLink": "https://calendar.google.com/event?eid=abc",
        "hangoutLink": "https://meet.google.com/abc-defg-hij",
        "eventType": "default", "recurringEventId": "series1",
        "recurrence": ["RRULE:FREQ=WEEKLY"], "visibility": "public",
        "created": "2026-11-01T09:00:00.000+09:00", "updated": "2026-11-02T00:00:00.000Z",
        "start": {"dateTime": "2026-11-20T10:00:00+09:00", "timeZone": "Asia/Tokyo"},
        "end": {"dateTime": "2026-11-20T11:00:00+09:00"},
        "organizer": {"email": "boss@example.com", "displayName": "Boss"},
        "creator": {"email": "boss@example.com"},
        "attendees": [
            {"email": "a@example.com", "responseStatus": "accepted"},
            {"email": "b@example.com", "displayName": "Bee", "responseStatus": "needsAction", "optional": true},
            {"email": "c@example.com", "responseStatus": "declined", "optional": false}
        ],
        "conferenceData": {
            "conferenceId": "abc-defg-hij",
            "conferenceSolution": {"key": {"type": "hangoutsMeet"}},
            "entryPoints": [
                {"entryPointType": "video", "uri": "https://meet.google.com/abc-defg-hij"},
                {"entryPointType": "phone", "uri": "tel:+1-555"}
            ]
        },
        "etag": "dropped"
    })
}

fn record() -> Value {
    json!({
        "id": "abc123", "summary": "Planning", "status": "confirmed",
        "start": "2026-11-20T01:00:00Z", "end": "2026-11-20T02:00:00Z",
        "time_zone": "Asia/Tokyo", "location": "Room 1",
        "meeting_code": "abc-defg-hij", "meeting_uri": "https://meet.google.com/abc-defg-hij",
        "html_link": "https://calendar.google.com/event?eid=abc",
        "organizer": {"name": "Boss", "email": "boss@example.com"},
        "creator": {"email": "boss@example.com"},
        "attendees": [
            {"email": "a@example.com", "response_status": "accepted"},
            {"name": "Bee", "email": "b@example.com", "response_status": "needsAction", "optional": true},
            {"email": "c@example.com", "response_status": "declined"}
        ],
        "recurrence": ["RRULE:FREQ=WEEKLY"], "event_type": "default",
        "visibility": "public", "recurring_event_id": "series1",
        "entry_points": [
            {"type": "video", "uri": "https://meet.google.com/abc-defg-hij"},
            {"type": "phone", "uri": "tel:+1-555"}
        ],
        "created": "2026-11-01T00:00:00Z", "updated": "2026-11-02T00:00:00Z",
        "description": "First line\r\nSecond line"
    })
}

async fn get(harness: &Harness, id: &str, json: bool) -> String {
    let mut out = Vec::new();
    calendar::get(&harness.api, id, json, &mut out)
        .await
        .unwrap();
    text(out)
}

#[tokio::test]
async fn text_and_json_are_two_renderings_of_one_curated_record() {
    let harness = Harness::start().await;
    harness
        .mount_json(&format!("{EVENTS_PATH}/abc123"), event())
        .await;
    let json = get(&harness, "abc123", true).await;
    assert_eq!(json, format!("{}\n", record()));
    let text = get(&harness, "abc123", false).await;
    assert_eq!(
        text,
        "id: abc123\nsummary: Planning\nstatus: confirmed\n\
         start: 2026-11-20T01:00:00Z\nend: 2026-11-20T02:00:00Z\n\
         time_zone: Asia/Tokyo\nlocation: Room 1\n\
         meeting_code: abc-defg-hij\nmeeting_uri: https://meet.google.com/abc-defg-hij\n\
         html_link: https://calendar.google.com/event?eid=abc\n\
         organizer: name=Boss email=boss@example.com\ncreator: email=boss@example.com\n\
         attendee: email=a@example.com response_status=accepted\n\
         attendee: name=Bee email=b@example.com response_status=needsAction optional=true\n\
         attendee: email=c@example.com response_status=declined\n\
         recurrence: RRULE:FREQ=WEEKLY\nevent_type: default\nvisibility: public\n\
         recurring_event_id: series1\n\
         entry_point: type=video uri=https://meet.google.com/abc-defg-hij\n\
         entry_point: type=phone uri=tel:+1-555\n\
         created: 2026-11-01T00:00:00Z\nupdated: 2026-11-02T00:00:00Z\n\
         description: First line\n  Second line\n"
    );
    assert_same_information(&text, &json);
    let requests = harness.requests(&format!("{EVENTS_PATH}/abc123")).await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["fields"],
        "id,status,summary,description,location,start,end,htmlLink,hangoutLink,eventType,recurringEventId,recurrence,visibility,created,updated,organizer(email,displayName),creator(email,displayName),attendees(email,displayName,responseStatus,optional),conferenceData(conferenceId,conferenceSolution(key(type)),entryPoints(entryPointType,uri)),attachments(title,mimeType,fileId)"
    );
}

fn attachments() -> Value {
    json!([
        {"fileUrl": "https://docs.google.com/document/d/1AbCdEf/edit", "title": "Notes by Gemini",
         "mimeType": "application/vnd.google-apps.document", "fileId": "1AbCdEf", "iconLink": "https://x.example/i.png"},
        {"fileUrl": "https://example.com/a.pdf", "title": "Agenda", "mimeType": "application/pdf"}
    ])
}

#[tokio::test]
async fn attachments_render_in_text_and_json_with_the_exact_mask() {
    let harness = Harness::start().await;
    let mut full = event();
    full["attachments"] = attachments();
    harness
        .mount_json(&format!("{EVENTS_PATH}/abc123"), full)
        .await;
    let json = get(&harness, "abc123", true).await;
    let keys: Vec<String> = serde_json::from_str::<Value>(&json)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let at = |name: &str| keys.iter().position(|key| key == name).unwrap();
    assert_eq!(at("attachments"), at("entry_points") + 1);
    assert_eq!(at("created"), at("attachments") + 1);
    assert!(json.contains(
        r#""attachments":[{"title":"Notes by Gemini","mime_type":"application/vnd.google-apps.document","file_id":"1AbCdEf"},{"title":"Agenda","mime_type":"application/pdf"}]"#
    ));
    let text = get(&harness, "abc123", false).await;
    assert!(
        text.contains(
            "entry_point: type=phone uri=tel:+1-555\n\
             attachment: title=\"Notes by Gemini\" mime_type=application/vnd.google-apps.document file_id=1AbCdEf\n\
             attachment: title=Agenda mime_type=application/pdf\n\
             created: "
        ),
        "{text}"
    );
    assert!(!text.contains("fileUrl") && !text.contains("iconLink") && !text.contains("x.example"));
    assert_same_information(&text, &json);
    for query in harness.requests(&format!("{EVENTS_PATH}/abc123")).await {
        assert!(
            query["fields"]
                .ends_with("entryPoints(entryPointType,uri)),attachments(title,mimeType,fileId)"),
            "{}",
            query["fields"]
        );
        assert!(!query["fields"].contains("fileUrl") && !query["fields"].contains("iconLink"));
    }
}

#[tokio::test]
async fn no_attachments_means_no_key() {
    let harness = Harness::start().await;
    for (id, attachments) in [
        ("none", Value::Null),
        ("empty", json!([])),
        (
            "blank",
            json!([{"fileUrl": "https://x.example/f", "iconLink": "https://x.example/i"}]),
        ),
    ] {
        let mut value =
            json!({"id": id, "start": {"date": "2026-11-20"}, "end": {"date": "2026-11-21"}});
        if !attachments.is_null() {
            value["attachments"] = attachments;
        }
        harness
            .mount_json(&format!("{EVENTS_PATH}/{id}"), value)
            .await;
        let json = get(&harness, id, true).await;
        assert_eq!(
            json,
            format!(
                "{}\n",
                json!({"id": id, "start": "2026-11-20", "end": "2026-11-21"})
            ),
            "{id}"
        );
        let text = get(&harness, id, false).await;
        assert!(!text.contains("attachment"), "{text}");
        assert_same_information(&text, &json);
    }
}

#[tokio::test]
async fn all_day_events_without_a_meet_link_keep_their_conference_id() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/day"),
            json!({"id": "day", "start": {"date": "2026-11-20"}, "end": {"date": "2026-11-21"},
                   "conferenceData": {"conferenceId": "zoom-1", "entryPoints": [{"entryPointType": "video", "uri": "https://zoom.example/1"}]}}),
        )
        .await;
    assert_eq!(
        get(&harness, "day", false).await,
        "id: day\nstart: 2026-11-20\nend: 2026-11-21\nconference_id: zoom-1\n\
         entry_point: type=video uri=https://zoom.example/1\n"
    );
}

#[tokio::test]
async fn reports_not_found() {
    let harness = Harness::start().await;
    let body = json!({"error": {"code": 404, "message": "Not Found"}}).to_string();
    harness.fail_all(404, &body).await;
    let mut out = Vec::new();
    let error = calendar::get(&harness.api, "missing", false, &mut out)
        .await
        .unwrap_err();
    assert!(gdcli::message(&error).contains("Not Found"));
    assert_eq!(out, b"");
}
