mod common;

use common::{EVENT_TYPES, EVENTS_PATH, Harness, at, cancelled, event, now, text};
use gdcli::calendar;
use serde_json::{Value, json};

const HEADER: &str = "id\twhen\tstart\tend\tsummary";

async fn render_all(harness: &Harness) -> Vec<String> {
    let mut out = Vec::new();
    calendar::list(&harness.api, now(), &mut out).await.unwrap();
    text(out).lines().map(str::to_owned).collect()
}

async fn render(harness: &Harness) -> Vec<String> {
    let mut lines = render_all(harness).await;
    assert_eq!(lines.remove(0), HEADER);
    lines
}

fn columns(line: &str) -> Vec<&str> {
    line.split('\t').collect()
}

fn past_windows(requests: &[std::collections::HashMap<String, String>]) -> Vec<String> {
    requests
        .iter()
        .filter(|query| query.contains_key("timeMax"))
        .map(|query| query["timeMin"].clone())
        .collect()
}

#[tokio::test]
async fn lists_five_past_then_five_upcoming_in_one_window() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (1..=7)
        .map(|day| {
            event(
                &format!("p{day}"),
                &format!("2026-11-{:02}T10:00:00Z", day + 10),
                &format!("2026-11-{:02}T11:00:00Z", day + 10),
            )
        })
        .collect();
    events.push(event(
        "ongoing",
        "2026-11-17T23:00:00Z",
        "2026-11-18T01:00:00Z",
    ));
    events.push(cancelled(
        "gone",
        "2026-11-17T12:00:00Z",
        "2026-11-17T13:00:00Z",
    ));
    events.extend((1..=6).map(|day| {
        event(
            &format!("u{day}"),
            &format!("2026-11-{:02}T10:00:00Z", day + 18),
            &format!("2026-11-{:02}T11:00:00Z", day + 18),
        )
    }));
    harness.mount_calendar(events).await;
    let lines = render(&harness).await;
    let ids: Vec<(&str, &str)> = lines
        .iter()
        .map(|line| {
            let c = columns(line);
            (c[1], c[0])
        })
        .collect();
    assert_eq!(
        ids,
        [
            ("past", "p3"),
            ("past", "p4"),
            ("past", "p5"),
            ("past", "p6"),
            ("past", "p7"),
            ("upcoming", "ongoing"),
            ("upcoming", "u1"),
            ("upcoming", "u2"),
            ("upcoming", "u3"),
            ("upcoming", "u4"),
        ]
    );
    assert_eq!(
        columns(&lines[0]),
        [
            "p3",
            "past",
            "2026-11-13T10:00:00Z",
            "2026-11-13T11:00:00Z",
            "Event p3"
        ]
    );
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(past_windows(&requests).len(), 1);
    for query in &requests {
        assert_eq!(query["singleEvents"], "true");
        assert_eq!(query["orderBy"], "startTime");
        assert_eq!(
            query["fields"],
            "nextPageToken,items(id,status,summary,start,end)"
        );
        let expected = if query.contains_key("timeMax") {
            "250"
        } else {
            "5"
        };
        assert_eq!(query["maxResults"], expected);
    }
}

#[tokio::test]
async fn widens_past_window_until_five_events_are_known() {
    let harness = Harness::start().await;
    let mut events = vec![event(
        "recent",
        "2026-11-15T10:00:00Z",
        "2026-11-15T11:00:00Z",
    )];
    events.extend((1..=5).map(|index| {
        event(
            &format!("old{index}"),
            &format!("2026-10-{:02}T10:00:00Z", index + 5),
            &format!("2026-10-{:02}T11:00:00Z", index + 5),
        )
    }));
    harness.mount_calendar(events).await;
    let lines = render(&harness).await;
    let past: Vec<&str> = lines.iter().map(|line| columns(line)[0]).collect();
    assert_eq!(past, ["old2", "old3", "old4", "old5", "recent"]);
    let windows = past_windows(&harness.requests(EVENTS_PATH).await);
    assert_eq!(
        windows,
        [
            "2026-11-11T00:00:00Z",
            "2026-10-19T00:00:00Z",
            "2026-05-22T00:00:00Z",
        ]
        .map(|value| common::millis(at(value)))
    );
}

#[tokio::test]
async fn searches_every_window_when_fewer_than_five_exist() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![event(
            "only",
            "2025-01-10T10:00:00Z",
            "2025-01-10T11:00:00Z",
        )])
        .await;
    let lines = render(&harness).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(columns(&lines[0])[0], "only");
    assert_eq!(past_windows(&harness.requests(EVENTS_PATH).await).len(), 4);
}

#[tokio::test]
async fn keeps_only_the_last_five_across_many_pages() {
    let harness = Harness::start().await;
    let first = at("2026-11-12T00:00:00Z");
    let events: Vec<Value> = (0..300)
        .map(|index| {
            let start = first + time::Duration::minutes(index * 20);
            let end = start + time::Duration::minutes(10);
            event(
                &format!("e{index:03}"),
                &common::rfc3339(start),
                &common::rfc3339(end),
            )
        })
        .collect();
    harness.mount_calendar(events).await;
    let lines = render(&harness).await;
    let past: Vec<&str> = lines
        .iter()
        .filter(|line| columns(line)[1] == "past")
        .map(|line| columns(line)[0])
        .collect();
    assert_eq!(past, ["e295", "e296", "e297", "e298", "e299"]);
    let paged = harness
        .requests(EVENTS_PATH)
        .await
        .into_iter()
        .filter(|query| query.contains_key("pageToken"))
        .count();
    assert_eq!(paged, 1);
}

#[tokio::test]
async fn pages_through_cancelled_upcoming_events() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (1..=5)
        .map(|index| {
            cancelled(
                &format!("c{index}"),
                &format!("2026-11-{:02}T10:00:00Z", index + 18),
                &format!("2026-11-{:02}T11:00:00Z", index + 18),
            )
        })
        .collect();
    events.extend((1..=5).map(|index| {
        event(
            &format!("ok{index}"),
            &format!("2026-12-{index:02}T10:00:00Z"),
            &format!("2026-12-{index:02}T11:00:00Z"),
        )
    }));
    harness.mount_calendar(events).await;
    let lines = render(&harness).await;
    let ids: Vec<&str> = lines.iter().map(|line| columns(line)[0]).collect();
    assert_eq!(ids, ["ok1", "ok2", "ok3", "ok4", "ok5"]);
}

#[tokio::test]
async fn prints_all_day_events_as_dates_and_sanitises_titles() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![
            json!({
                "id": "holiday",
                "status": "confirmed",
                "summary": "Line one\nLine\ttwo",
                "start": {"date": "2026-11-20"},
                "end": {"date": "2026-11-21"},
            }),
            json!({
                "id": "leave",
                "status": "confirmed",
                "start": {"date": "2026-11-16"},
                "end": {"date": "2026-11-17"},
            }),
        ])
        .await;
    let lines = render(&harness).await;
    assert_eq!(
        lines,
        [
            "leave\tpast\t2026-11-16\t2026-11-17\t",
            "holiday\tupcoming\t2026-11-20\t2026-11-21\tLine one Line two",
        ]
    );
}

#[tokio::test]
async fn first_line_is_the_header_with_as_many_columns_as_every_row() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![
            event("p1", "2026-11-17T10:00:00Z", "2026-11-17T11:00:00Z"),
            event("u1", "2026-11-19T10:00:00Z", "2026-11-19T11:00:00Z"),
        ])
        .await;
    let lines = render_all(&harness).await;
    assert_eq!(
        lines,
        [
            HEADER,
            "p1\tpast\t2026-11-17T10:00:00Z\t2026-11-17T11:00:00Z\tEvent p1",
            "u1\tupcoming\t2026-11-19T10:00:00Z\t2026-11-19T11:00:00Z\tEvent u1",
        ]
    );
    for line in &lines {
        assert_eq!(columns(line).len(), columns(HEADER).len(), "{line}");
    }
}

#[tokio::test]
async fn header_is_printed_when_there_are_no_events() {
    let harness = Harness::start().await;
    harness.mount_calendar(Vec::new()).await;
    let mut out = Vec::new();
    calendar::list(&harness.api, now(), &mut out).await.unwrap();
    assert_eq!(text(out), format!("{HEADER}\n"));
}

#[tokio::test]
async fn every_list_request_asks_only_for_appointment_like_event_types() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![event(
            "u1",
            "2026-11-19T10:00:00Z",
            "2026-11-19T11:00:00Z",
        )])
        .await;
    render(&harness).await;
    let types = harness.query_values(EVENTS_PATH, "eventTypes").await;
    assert!(types.len() >= 2);
    for requested in types {
        assert_eq!(requested, EVENT_TYPES);
    }
}

#[tokio::test]
async fn nothing_is_printed_when_the_api_fails() {
    let harness = Harness::start().await;
    let mut out = Vec::new();
    assert!(calendar::list(&harness.api, now(), &mut out).await.is_err());
    assert_eq!(out, b"");
}

#[tokio::test]
async fn drops_ended_all_day_events_from_upcoming_so_nothing_prints_twice() {
    let harness = Harness::start().await;
    harness
        .mount_json(
            EVENTS_PATH,
            json!({"items": [
                {"id": "allday", "status": "confirmed", "start": {"date": "2026-11-17"}, "end": {"date": "2026-11-18"}},
                {"id": "later", "status": "confirmed", "start": {"date": "2026-11-20"}, "end": {"date": "2026-11-21"}}
            ]}),
        )
        .await;
    let lines = render(&harness).await;
    assert_eq!(
        lines,
        [
            "allday\tpast\t2026-11-17\t2026-11-18\t",
            "later\tupcoming\t2026-11-20\t2026-11-21\t"
        ]
    );
}

#[tokio::test]
async fn propagates_api_errors() {
    let harness = Harness::start().await;
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(
            wiremock::ResponseTemplate::new(403)
                .set_body_json(json!({"error": {"code": 403, "message": "Calendar API disabled"}})),
        )
        .mount(&harness.server)
        .await;
    let mut out = Vec::new();
    let error = calendar::list(&harness.api, now(), &mut out)
        .await
        .unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 403: Calendar API disabled"
    );
}
