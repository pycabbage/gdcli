mod common;

use common::{EVENT_TYPES, EVENTS_PATH, Harness, at, cancelled, event, now, text};
use gdcli::calendar;
use serde_json::{Value, json};

const HEADER: &str = "id\twhen\tstart\tend\tsummary";

async fn render_all(harness: &Harness) -> Vec<String> {
    let mut out = Vec::new();
    calendar::list(&harness.api, now(), &common::args(), &mut out)
        .await
        .unwrap();
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
    calendar::list(&harness.api, now(), &common::args(), &mut out)
        .await
        .unwrap();
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
    assert!(
        calendar::list(&harness.api, now(), &common::args(), &mut out)
            .await
            .is_err()
    );
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
    let body = json!({"error": {"code": 403, "message": "Calendar API disabled"}}).to_string();
    harness.fail_all(403, &body).await;
    let mut out = Vec::new();
    let error = calendar::list(&harness.api, now(), &common::args(), &mut out)
        .await
        .unwrap_err();
    assert_eq!(
        gdcli::message(&error),
        "google api error 403: Calendar API disabled"
    );
}

fn dated(id: &str, days_ago: i64) -> Value {
    let start = now() - time::Duration::days(days_ago) - time::Duration::hours(2);
    event(
        id,
        &common::rfc3339(start),
        &common::rfc3339(start + time::Duration::hours(1)),
    )
}

fn ahead(id: &str, hours: i64) -> Value {
    let start = now() + time::Duration::hours(hours);
    event(
        id,
        &common::rfc3339(start),
        &common::rfc3339(start + time::Duration::minutes(30)),
    )
}

async fn run(harness: &Harness, past: u16, upcoming: u16, json: bool) -> String {
    let args = gdcli::cli::ListArgs {
        past,
        upcoming,
        json,
    };
    let mut out = Vec::new();
    calendar::list(&harness.api, now(), &args, &mut out)
        .await
        .unwrap();
    text(out)
}

fn ids(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .map(|line| columns(line)[0].to_owned())
        .collect()
}

#[tokio::test]
async fn the_requested_counts_are_honoured_on_both_sides() {
    let mut events: Vec<Value> = (1..=12)
        .map(|day| dated(&format!("p{day:02}"), day))
        .collect();
    events.extend((1..=12).map(|hour| ahead(&format!("u{hour:02}"), hour)));
    for (past, upcoming) in [(1, 1), (3, 8), (8, 3), (12, 12)] {
        let harness = Harness::start().await;
        harness.mount_calendar(events.clone()).await;
        let found = ids(&run(&harness, past, upcoming, false).await);
        let expected: Vec<String> = (1..=past)
            .rev()
            .map(|day| format!("p{day:02}"))
            .chain((1..=upcoming).map(|hour| format!("u{hour:02}")))
            .collect();
        assert_eq!(found, expected, "{past} {upcoming}");
        let upcoming_size = upcoming.to_string();
        let sizes: Vec<String> = harness
            .requests(EVENTS_PATH)
            .await
            .into_iter()
            .filter(|query| !query.contains_key("timeMax"))
            .map(|query| query["maxResults"].clone())
            .collect();
        assert_eq!(sizes, [upcoming_size], "{upcoming}");
    }
}

#[tokio::test]
async fn zero_skips_a_side_without_any_request_for_it() {
    let events = vec![dated("p1", 1), ahead("u1", 1)];
    for (past, upcoming, past_requests, upcoming_requests) in
        [(0, 5, 0, 1), (5, 0, 4, 0), (0, 0, 0, 0)]
    {
        let harness = Harness::start().await;
        harness.mount_calendar(events.clone()).await;
        let output = run(&harness, past, upcoming, false).await;
        let requests = harness.requests(EVENTS_PATH).await;
        let with_max = requests
            .iter()
            .filter(|query| query.contains_key("timeMax"))
            .count();
        assert_eq!(with_max, past_requests, "{past} {upcoming}");
        assert_eq!(
            requests.len() - with_max,
            upcoming_requests,
            "{past} {upcoming}"
        );
        assert_eq!(
            output.lines().count(),
            1 + usize::from(past > 0) + usize::from(upcoming > 0)
        );
    }
}

#[tokio::test]
async fn larger_past_counts_widen_the_windows_and_keep_only_the_newest() {
    let harness = Harness::start().await;
    let mut events: Vec<Value> = (1..=3).map(|day| dated(&format!("a{day}"), day)).collect();
    events.extend((1..=3).map(|day| dated(&format!("b{day}"), 10 + day)));
    events.extend((1..=4).map(|day| dated(&format!("c{day}"), 100 + day)));
    harness.mount_calendar(events).await;
    let found = ids(&run(&harness, 8, 0, false).await);
    assert_eq!(found, ["c2", "c1", "b3", "b2", "b1", "a3", "a2", "a1"]);
    let windows = past_windows(&harness.requests(EVENTS_PATH).await);
    assert_eq!(windows.len(), 3);
}

#[tokio::test]
async fn json_lists_are_arrays_of_objects_keyed_by_the_header_columns() {
    let harness = Harness::start().await;
    harness
        .mount_calendar(vec![
            event("p1", "2026-11-17T10:00:00Z", "2026-11-17T11:00:00Z"),
            event("u1", "2026-11-19T10:00:00Z", "2026-11-19T11:00:00Z"),
        ])
        .await;
    let json = run(&harness, 5, 5, true).await;
    assert_eq!(
        json,
        "[{\"id\":\"p1\",\"when\":\"past\",\"start\":\"2026-11-17T10:00:00Z\",\"end\":\"2026-11-17T11:00:00Z\",\"summary\":\"Event p1\"},\
         {\"id\":\"u1\",\"when\":\"upcoming\",\"start\":\"2026-11-19T10:00:00Z\",\"end\":\"2026-11-19T11:00:00Z\",\"summary\":\"Event u1\"}]\n"
    );
    let rows: Vec<Value> = serde_json::from_str(&json).unwrap();
    let text = run(&harness, 5, 5, false).await;
    for (row, line) in rows.iter().zip(text.lines().skip(1)) {
        let cells: Vec<&str> = HEADER
            .split('\t')
            .map(|key| row[key].as_str().unwrap())
            .collect();
        assert_eq!(cells.join("\t"), line);
    }
    assert_eq!(
        run(&Harness::start_with(Vec::new()).await, 5, 5, true).await,
        "[]\n"
    );
}

#[tokio::test]
async fn calendar_upcoming_pages_never_exceed_250_events() {
    for (requested, expected) in [
        (1, "1"),
        (100, "100"),
        (250, "250"),
        (400, "250"),
        (1000, "250"),
    ] {
        let harness = Harness::start_with(vec![ahead("u1", 1)]).await;
        let args = gdcli::cli::ListArgs {
            past: 0,
            upcoming: requested,
            json: false,
        };
        calendar::list(&harness.api, now(), &args, &mut Vec::new())
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
