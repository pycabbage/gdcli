mod common;

use common::{EVENTS_PATH, Harness, text};
use gdcli::calendar;
use serde_json::json;

#[tokio::test]
async fn prints_only_non_empty_fields_with_indented_multiline_values() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            &format!("{EVENTS_PATH}/abc123"),
            json!({
                "id": "abc123",
                "status": "confirmed",
                "summary": "Planning",
                "description": "First line\nSecond line",
                "location": "Room 1",
                "start": {"dateTime": "2026-11-20T10:00:00+09:00", "timeZone": "Asia/Tokyo"},
                "end": {"dateTime": "2026-11-20T11:00:00+09:00"},
                "organizer": {"email": "boss@example.com", "displayName": "Boss"},
                "attendees": [
                    {"email": "a@example.com", "responseStatus": "accepted"},
                    {"email": "b@example.com", "displayName": "Bee", "responseStatus": "needsAction"}
                ],
                "hangoutLink": "https://meet.google.com/abc-defg-hij",
                "conferenceData": {
                    "conferenceId": "abc-defg-hij",
                    "entryPoints": [{"entryPointType": "video", "uri": "https://meet.google.com/abc-defg-hij"}]
                },
                "created": "2026-11-01T00:00:00.000Z"
            }),
        )
        .await;
    let mut out = Vec::new();
    calendar::get(&harness.api, "abc123", &mut out)
        .await
        .unwrap();
    assert_eq!(
        text(out),
        "id: abc123\n\
         status: confirmed\n\
         summary: Planning\n\
         start: 2026-11-20T01:00:00Z\n\
         end: 2026-11-20T02:00:00Z\n\
         time_zone: Asia/Tokyo\n\
         location: Room 1\n\
         organizer: Boss <boss@example.com>\n\
         attendee: a@example.com [accepted]\n\
         attendee: Bee <b@example.com> [needsAction]\n\
         meet_link: https://meet.google.com/abc-defg-hij\n\
         conference_id: abc-defg-hij\n\
         entry_point_video: https://meet.google.com/abc-defg-hij\n\
         created: 2026-11-01T00:00:00Z\n\
         description: First line\n  Second line\n"
    );
    let requests = harness.requests(&format!("{EVENTS_PATH}/abc123")).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["fields"],
        "id,status,summary,description,location,start,end,htmlLink,hangoutLink,eventType,recurringEventId,recurrence,visibility,created,updated,organizer(email,displayName),creator(email,displayName),attendees(email,displayName,responseStatus,optional),conferenceData(conferenceId,conferenceSolution(key(type)),entryPoints(entryPointType,uri))"
    );
}

#[tokio::test]
async fn reports_not_found() {
    let harness = Harness::start().await;
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(
            wiremock::ResponseTemplate::new(404)
                .set_body_json(json!({"error": {"code": 404, "message": "Not Found"}})),
        )
        .mount(&harness.server)
        .await;
    let mut out = Vec::new();
    let error = calendar::get(&harness.api, "missing", &mut out)
        .await
        .unwrap_err();
    assert!(gdcli::message(&error).contains("Not Found"));
    assert!(out.is_empty());
}
