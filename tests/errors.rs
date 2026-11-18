mod common;

use common::{EVENTS_PATH, Harness, meet_event, now, text};
use gdcli::google::with_deadline;
use gdcli::{calendar, docs, meet};
use serde_json::json;
use std::time::{Duration, Instant};
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

const HTML: &str =
    "<html><body><h1>Not Found</h1><p>The requested URL was not found.</p></body></html>";

#[tokio::test]
async fn non_json_error_bodies_print_only_the_status() {
    let harness = Harness::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(404).set_body_string(HTML))
        .mount(&harness.server)
        .await;
    let error = calendar::get(&harness.api, "x", &mut Vec::new())
        .await
        .unwrap_err();
    assert_eq!(gdcli::message(&error), "google api error 404 Not Found");
    let error = docs::get(&harness.api, "https://example.com/x", &mut Vec::new())
        .await
        .unwrap_err();
    assert_eq!(gdcli::message(&error), "google api error 404 Not Found");
}

#[tokio::test]
async fn meet_errors_never_echo_the_response_body() {
    for body in [HTML.to_owned(), json!({"unexpected": true}).to_string()] {
        let harness = Harness::start().await;
        harness
            .mount_json(
                &format!("{EVENTS_PATH}/evt1"),
                meet_event(
                    "evt1",
                    "abc-defg-hij",
                    "2026-11-17T01:00:00Z",
                    "2026-11-17T02:00:00Z",
                ),
            )
            .await;
        Mock::given(any())
            .and(wiremock::matchers::path_regex("^/v2/"))
            .respond_with(ResponseTemplate::new(502).set_body_string(body))
            .mount(&harness.server)
            .await;
        let error = meet::get(&harness.api, "evt1", &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(gdcli::message(&error), "google api error 502 Bad Gateway");
    }
}

#[tokio::test]
async fn meet_path_segments_are_encoded_one_per_id() {
    for code in ["a/b", "x/../../etc", "a?b#c", "a%2Fb"] {
        let harness = Harness::start().await;
        harness
            .mount_json(
                &format!("{EVENTS_PATH}/evt1"),
                meet_event("evt1", code, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
            )
            .await;
        let _ = meet::get(&harness.api, "evt1", &mut Vec::new()).await;
        let meet_requests: Vec<_> = harness
            .server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| !request.url.path().starts_with("/calendar/"))
            .collect();
        assert!(!meet_requests.is_empty(), "{code}");
        for request in meet_requests {
            let segments: Vec<&str> = request.url.path_segments().unwrap().collect();
            assert_eq!(
                segments.len(),
                if request.url.path().ends_with("conferenceRecords") {
                    2
                } else {
                    3
                },
                "{code}: {}",
                request.url
            );
            assert_eq!(segments[0], "v2", "{code}");
            assert!(
                segments
                    .iter()
                    .all(|segment| *segment != ".." && *segment != "."),
                "{code}: {}",
                request.url
            );
        }
    }
}

#[tokio::test]
async fn a_command_that_never_gets_an_answer_fails_at_the_deadline() {
    let harness = Harness::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&harness.server)
        .await;
    let started = Instant::now();
    let error = with_deadline(
        Some(Duration::from_secs(1)),
        calendar::list(&harness.api, now(), &mut Vec::new()),
    )
    .await
    .unwrap_err();
    assert_eq!(gdcli::message(&error), "timed out after 1s");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn the_deadline_passes_results_through() {
    let harness = Harness::start().await;
    harness.mount_calendar(Vec::new()).await;
    let mut out = Vec::new();
    with_deadline(
        Some(Duration::from_secs(30)),
        calendar::list(&harness.api, now(), &mut out),
    )
    .await
    .unwrap();
    assert!(text(out).starts_with("id\twhen"));
}

#[tokio::test]
async fn without_a_deadline_the_future_runs_to_completion() {
    let result = with_deadline(None, async {
        tokio::task::yield_now().await;
        Ok(7)
    })
    .await;
    assert_eq!(result.unwrap(), 7);
}

#[test]
fn production_limits_are_ten_seconds_to_connect_and_two_minutes_overall() {
    assert_eq!(gdcli::google::CONNECT_TIMEOUT, Duration::from_secs(10));
    assert_eq!(gdcli::google::DEADLINE, Duration::from_secs(120));
}

#[tokio::test]
async fn dot_segments_are_rejected_before_their_request() {
    for code in [".", ".."] {
        let harness = Harness::start().await;
        harness
            .mount_json(
                &format!("{EVENTS_PATH}/evt1"),
                meet_event("evt1", code, "2026-11-17T01:00:00Z", "2026-11-17T02:00:00Z"),
            )
            .await;
        let error = meet::get(&harness.api, "evt1", &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(gdcli::message(&error), format!("invalid id \"{code}\""));
        assert!(
            harness
                .server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|request| !request.url.path().starts_with("/v2/spaces"))
        );
    }
}

#[tokio::test]
async fn unusual_error_and_response_bodies_map_to_fixed_short_messages() {
    for (status, body, expected) in [
        (
            400,
            r#"{"error": {"code": 400}}"#,
            "google api error 400 Bad Request",
        ),
        (
            500,
            r#"{"unexpected": [1, 2, 3]}"#,
            "google api error 500 Internal Server Error",
        ),
        (503, "", "google api error 503 Service Unavailable"),
    ] {
        let harness = Harness::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&harness.server)
            .await;
        let error = calendar::get(&harness.api, "x", &mut Vec::new())
            .await
            .unwrap_err();
        assert_eq!(gdcli::message(&error), expected);
    }
    let harness = Harness::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>secret page body</html>"))
        .mount(&harness.server)
        .await;
    let error = calendar::get(&harness.api, "x", &mut Vec::new())
        .await
        .unwrap_err();
    let message = gdcli::message(&error);
    assert!(
        message.starts_with("unexpected google api response: "),
        "{message}"
    );
    assert!(!message.contains("secret page body"), "{message}");
}
