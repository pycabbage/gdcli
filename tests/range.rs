mod common;

use clap::Parser;
use common::{
    EVENT_TYPES, EVENTS_PATH, Harness, at, cancelled, event, meet_event, now, text, titled,
};
use gdcli::cli::{CalendarCommand, Cli, Command, ListArgs, MeetCommand};
use gdcli::{calendar, meet};
use serde_json::{Value, json};
use time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const CALENDAR_HEADER: &str = "id\twhen\tstart\tend\tsummary";
const MEET_HEADER: &str = "id\twhen\tstart\tend\tmeeting_code\tsummary";
const CALENDAR_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end)";
const MEET_FIELDS: &str = "nextPageToken,items(id,status,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))))";

fn parsed(group: &str, extra: &[&str]) -> ListArgs {
    let args = ["gdcli", group, "list"]
        .into_iter()
        .chain(extra.iter().copied());
    match Cli::try_parse_from(args).unwrap().command {
        Command::Calendar(CalendarCommand::List(args)) | Command::Meet(MeetCommand::List(args)) => {
            args
        }
        other => panic!("{other:?}"),
    }
}

async fn calendar_text(harness: &Harness, extra: &[&str]) -> String {
    let mut out = Vec::new();
    calendar::list(&harness.api, now(), &parsed("calendar", extra), &mut out)
        .await
        .unwrap();
    text(out)
}

async fn meet_text(harness: &Harness, extra: &[&str]) -> String {
    let mut out = Vec::new();
    meet::list(&harness.api, now(), &parsed("meet", extra), &mut out)
        .await
        .unwrap();
    text(out)
}

fn ids(output: &str) -> Vec<&str> {
    output
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next().unwrap())
        .collect()
}

fn hourly(count: usize, make: impl Fn(usize, &str, &str) -> Value) -> Vec<Value> {
    let first = at("2026-11-01T00:00:00Z");
    (0..count)
        .map(|index| {
            let start = first + Duration::hours(i64::try_from(index).unwrap());
            make(
                index,
                &common::rfc3339(start),
                &common::rfc3339(start + Duration::minutes(30)),
            )
        })
        .collect()
}

fn day_event(id: &str, title: &str, day: u32, hour: u32) -> Value {
    titled(
        id,
        title,
        &format!("2026-11-{day:02}T{hour:02}:00:00Z"),
        &format!("2026-11-{day:02}T{:02}:00:00Z", hour + 1),
    )
}

fn plain(count: usize) -> Vec<Value> {
    hourly(count, |index, start, end| {
        event(&format!("e{index:03}"), start, end)
    })
}

#[tokio::test]
async fn range_sends_timemin_timemax_q_and_the_page_size() {
    let wanted = "weekly sync & 100%+x=y \u{e9}";
    let harness = Harness::start_with(vec![
        titled("a", wanted, "2026-11-03T01:00:00Z", "2026-11-03T02:00:00Z"),
        titled("b", "Other", "2026-11-04T01:00:00Z", "2026-11-04T02:00:00Z"),
    ])
    .await;
    let output = calendar_text(
        &harness,
        &[
            "--from",
            "2026-11-01",
            "--to",
            "2026-11-10",
            "--query",
            wanted,
            "--limit",
            "7",
        ],
    )
    .await;
    assert_eq!(
        output,
        format!(
            "{CALENDAR_HEADER}\na\tpast\t2026-11-03T01:00:00Z\t2026-11-03T02:00:00Z\t{wanted}\n"
        )
    );
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 1);
    let query = &requests[0];
    assert_eq!(query["timeMin"], "2026-11-01T00:00:00.000Z");
    assert_eq!(query["timeMax"], "2026-11-10T00:00:00.000Z");
    assert_eq!(query["q"], wanted);
    assert_eq!(query["maxResults"], "7");
    assert_eq!(query["singleEvents"], "true");
    assert_eq!(query["orderBy"], "startTime");
    assert_eq!(query["fields"], CALENDAR_FIELDS);
    assert!(!query.contains_key("pageToken"));
    let received = harness.server.received_requests().await.unwrap();
    let raw = received[0].url.query().unwrap();
    assert!(
        raw.contains("q=weekly+sync+%26+100%25%2Bx%3Dy+%C3%A9"),
        "{raw}"
    );
}

#[tokio::test]
async fn a_bare_date_is_utc_midnight_and_offsets_are_converted() {
    for (from, to, min, max) in [
        (
            "2026-11-01",
            "2026-11-05T09:00:00+09:00",
            "2026-11-01T00:00:00.000Z",
            "2026-11-05T00:00:00.000Z",
        ),
        (
            "2026-11-01T00:00:00+09:00",
            "2026-11-05T00:00:00Z",
            "2026-10-31T15:00:00.000Z",
            "2026-11-05T00:00:00.000Z",
        ),
        (
            "2026-11-01T10:20:30.789-05:00",
            "2026-11-02",
            "2026-11-01T15:20:30.000Z",
            "2026-11-02T00:00:00.000Z",
        ),
    ] {
        let harness = Harness::start_with(Vec::new()).await;
        calendar_text(&harness, &["--from", from, "--to", to]).await;
        let requests = harness.requests(EVENTS_PATH).await;
        assert_eq!(requests.len(), 1, "{from}");
        assert_eq!(requests[0]["timeMin"], min, "{from}");
        assert_eq!(requests[0]["timeMax"], max, "{to}");
    }
}

#[tokio::test]
async fn from_only_sends_no_time_max() {
    let harness = Harness::start_with(plain(3)).await;
    for output in [
        calendar_text(&harness, &["--from", "2026-11-01"]).await,
        meet_text(&harness, &["--from", "2026-11-01"]).await,
    ] {
        assert!(output.starts_with("id\twhen\t"), "{output}");
    }
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 2);
    for query in requests {
        assert_eq!(query["timeMin"], "2026-11-01T00:00:00.000Z");
        assert!(!query.contains_key("timeMax"));
        assert!(!query.contains_key("q"));
    }
}

#[tokio::test]
async fn rows_are_ordered_by_start_with_when_by_end_time() {
    let harness = Harness::start_with(vec![
        event("later", "2026-11-20T00:00:00Z", "2026-11-20T01:00:00Z"),
        event("short", "2026-11-12T00:00:00Z", "2026-11-12T01:00:00Z"),
        event("long", "2026-11-10T00:00:00Z", "2026-11-19T00:00:00Z"),
        event("first", "2026-11-05T00:00:00Z", "2026-11-05T01:00:00Z"),
    ])
    .await;
    let output = calendar_text(&harness, &["--from", "2026-11-01"]).await;
    let rows: Vec<(&str, &str)> = output
        .lines()
        .skip(1)
        .map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            (columns[0], columns[1])
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("first", "past"),
            ("long", "upcoming"),
            ("short", "past"),
            ("later", "upcoming"),
        ]
    );
}

#[tokio::test]
async fn paging_stops_at_the_limit() {
    for (limit, requests, size) in [
        (10, 1, "10"),
        (250, 1, "250"),
        (251, 2, "250"),
        (600, 3, "250"),
    ] {
        let harness = Harness::start_with(plain(600)).await;
        let output = calendar_text(
            &harness,
            &["--from", "2026-11-01", "--limit", &limit.to_string()],
        )
        .await;
        let found = ids(&output);
        assert_eq!(found.len(), limit, "{limit}");
        assert_eq!(found[0], "e000");
        assert_eq!(found[limit - 1], format!("e{:03}", limit - 1));
        let sent = harness.requests(EVENTS_PATH).await;
        assert_eq!(sent.len(), requests, "{limit}");
        assert!(sent.iter().all(|query| query["maxResults"] == size));
        if requests > 1 {
            assert_eq!(sent[1]["pageToken"], "250");
        }
    }
}

#[tokio::test]
async fn limit_keeps_the_oldest_matches_not_the_newest() {
    let harness = Harness::start_with(hourly(10, |index, start, end| {
        let title = if index % 2 == 0 { "standup" } else { "other" };
        titled(&format!("e{index}"), title, start, end)
    }))
    .await;
    assert_eq!(
        ids(&calendar_text(&harness, &["--from", "2026-11-01", "--limit", "3"]).await),
        ["e0", "e1", "e2"]
    );
    assert_eq!(
        ids(&calendar_text(
            &harness,
            &["--from", "2026-11-01", "--limit", "3", "--query", "standup"]
        )
        .await),
        ["e0", "e2", "e4"]
    );
}

#[tokio::test]
async fn cancelled_events_are_dropped_and_event_types_are_sent() {
    let harness = Harness::start_with(hourly(6, |index, start, end| {
        let make = if index % 2 == 0 { cancelled } else { event };
        make(&format!("e{index}"), start, end)
    }))
    .await;
    let output = calendar_text(&harness, &["--from", "2026-11-01", "--limit", "2"]).await;
    assert_eq!(ids(&output), ["e1", "e3"]);
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|query| query["maxResults"] == "2"));
    for types in harness.query_values(EVENTS_PATH, "eventTypes").await {
        assert_eq!(types, EVENT_TYPES);
    }
}

#[tokio::test]
async fn the_default_limit_is_50() {
    let harness = Harness::start_with(plain(120)).await;
    let output = calendar_text(&harness, &["--from", "2026-11-01"]).await;
    let found = ids(&output);
    assert_eq!(found.len(), 50);
    assert_eq!((found[0], found[49]), ("e000", "e049"));
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["maxResults"], "50");
}

#[tokio::test]
async fn meet_range_keeps_only_meet_events_with_page_size_250() {
    let meetings = || {
        hourly(600, |index, start, end| {
            let id = format!("e{index:03}");
            if index % 4 == 0 {
                meet_event(&id, "aaa-bbbb-ccc", start, end)
            } else {
                event(&id, start, end)
            }
        })
    };
    let harness = Harness::start_with(meetings()).await;
    let output = meet_text(&harness, &["--from", "2026-11-01", "--limit", "5"]).await;
    assert!(output.starts_with(&format!("{MEET_HEADER}\n")));
    assert_eq!(ids(&output), ["e000", "e004", "e008", "e012", "e016"]);
    for line in output.lines().skip(1) {
        assert_eq!(line.split('\t').nth(4), Some("aaa-bbbb-ccc"), "{line}");
    }
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["maxResults"], "250");
    assert_eq!(requests[0]["fields"], MEET_FIELDS);
    let harness = Harness::start_with(meetings()).await;
    let output = meet_text(&harness, &["--from", "2026-11-01", "--limit", "100"]).await;
    assert_eq!(ids(&output).len(), 100);
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|query| query["maxResults"] == "250"));
}

#[tokio::test]
async fn range_json_equals_the_text_rows() {
    let mut events = plain(4);
    events.push(titled(
        "tabbed",
        "a\tb\nc",
        "2026-11-02T00:00:00Z",
        "2026-11-02T01:00:00Z",
    ));
    events.push(meet_event(
        "meeting",
        "aaa-bbbb-ccc",
        "2026-11-03T00:00:00Z",
        "2026-11-03T01:00:00Z",
    ));
    let harness = Harness::start_with(events).await;
    for (header, group) in [(CALENDAR_HEADER, "calendar"), (MEET_HEADER, "meet")] {
        let run = |json: bool| {
            let harness = &harness;
            async move {
                let mut extra = vec!["--from", "2026-11-01", "--limit", "20"];
                extra.extend(json.then_some("--json"));
                if group == "calendar" {
                    calendar_text(harness, &extra).await
                } else {
                    meet_text(harness, &extra).await
                }
            }
        };
        let (rows, json) = (run(false).await, run(true).await);
        assert_eq!(rows.lines().next(), Some(header));
        let objects: Vec<Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(objects.len(), rows.lines().count() - 1);
        for (line, object) in rows.lines().skip(1).zip(&objects) {
            let cells: Vec<&str> = line.split('\t').collect();
            let keys: Vec<&str> = header.split('\t').collect();
            assert_eq!(object.as_object().unwrap().keys().collect::<Vec<_>>(), keys);
            for (key, cell) in keys.iter().zip(cells) {
                assert_eq!(
                    object[key].as_str().unwrap().replace(['\t', '\n'], " "),
                    cell
                );
            }
        }
    }
}

#[test]
fn check_rejects_from_not_before_to_and_equal_bounds() {
    let message = |extra: &[&str]| {
        parsed("calendar", extra)
            .check()
            .map_err(|error| error.to_string())
    };
    for extra in [
        &["--from", "2026-11-10", "--to", "2026-11-01"][..],
        &["--from", "2026-11-01", "--to", "2026-11-01"],
        &["--from", "2026-11-01", "--to", "2026-11-01T00:00:00Z"],
        &[
            "--from",
            "2026-11-01T00:00:00.1Z",
            "--to",
            "2026-11-01T00:00:00.9Z",
        ],
        &[
            "--from",
            "2026-11-01T00:00:00.9Z",
            "--to",
            "2026-11-01T00:00:00.1Z",
        ],
        &[
            "--from",
            "2026-11-01T09:00:00Z",
            "--to",
            "2026-11-01T18:00:00+09:00",
        ],
    ] {
        assert_eq!(
            message(extra).unwrap_err(),
            "invalid range: --from must be earlier than --to",
            "{extra:?}"
        );
    }
    for extra in [
        &[][..],
        &["--from", "2026-11-01"],
        &["--from", "2026-11-01", "--to", "2026-11-01T00:00:01Z"],
        &[
            "--from",
            "2026-11-01T00:00:00.9Z",
            "--to",
            "2026-11-01T00:00:01.1Z",
        ],
        &["--from", "2026-11-01T00:00:00+09:00", "--to", "2026-11-01"],
        &["--query", "x", "--past", "1"],
    ] {
        assert!(message(extra).is_ok(), "{extra:?}");
    }
    let mut args = parsed("meet", &["--from", "2026-11-02"]);
    args.to = Some(at("2026-11-01T00:00:00Z"));
    assert!(args.check().is_err());
    args.from = None;
    assert!(args.check().is_ok());
}

#[tokio::test]
async fn empty_range_prints_the_header_or_an_empty_array() {
    let harness = Harness::start_with(Vec::new()).await;
    assert_eq!(
        calendar_text(&harness, &["--from", "2026-11-01"]).await,
        format!("{CALENDAR_HEADER}\n")
    );
    assert_eq!(
        meet_text(&harness, &["--from", "2026-11-01"]).await,
        format!("{MEET_HEADER}\n")
    );
    for text in [
        calendar_text(&harness, &["--from", "2026-11-01", "--json"]).await,
        meet_text(&harness, &["--from", "2026-11-01", "--json"]).await,
    ] {
        assert_eq!(text, "[]\n");
    }
}

#[tokio::test]
async fn api_failure_prints_nothing() {
    let failing = Harness::start().await;
    failing.fail_all(500, "{}").await;
    let partial = Harness::start().await;
    Mock::given(method("GET"))
        .and(path(EVENTS_PATH))
        .and(query_param("pageToken", "next"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({"error": {"message": "late"}})),
        )
        .mount(&partial.server)
        .await;
    partial
        .mount_json(
            EVENTS_PATH,
            json!({"items": [event("e1", "2026-11-02T00:00:00Z", "2026-11-02T01:00:00Z")], "nextPageToken": "next"}),
        )
        .await;
    let args = parsed("calendar", &["--from", "2026-11-01", "--limit", "5"]);
    for harness in [&failing, &partial] {
        let mut out = Vec::new();
        assert!(
            calendar::list(&harness.api, now(), &args, &mut out)
                .await
                .is_err()
        );
        assert_eq!(out, b"");
        let mut out = Vec::new();
        let args = parsed("meet", &["--from", "2026-11-01"]);
        assert!(
            meet::list(&harness.api, now(), &args, &mut out)
                .await
                .is_err()
        );
        assert_eq!(out, b"");
    }
}

#[tokio::test]
async fn query_is_added_to_every_default_window_request() {
    let mut events = vec![
        day_event("p1", "standup A", 14, 1),
        day_event("p2", "Standup B", 15, 1),
        day_event("u1", "standup C", 20, 1),
    ];
    events.extend((0..20).map(|index| {
        day_event(
            &format!("n{index}"),
            &format!("Event n{index}"),
            1 + index,
            5,
        )
    }));
    let harness = Harness::start_with(events).await;
    let output = calendar_text(&harness, &["--query", "standup", "--upcoming", "3"]).await;
    assert_eq!(ids(&output), ["p1", "p2", "u1"]);
    let requests = harness.requests(EVENTS_PATH).await;
    assert_eq!(requests.len(), 5);
    assert!(requests.iter().all(|query| query["q"] == "standup"));
    let windows: Vec<String> = requests
        .iter()
        .filter(|query| query.contains_key("timeMax"))
        .map(|query| query["timeMin"].clone())
        .collect();
    assert_eq!(
        windows,
        [
            "2026-11-11T00:00:00.000Z",
            "2026-10-19T00:00:00.000Z",
            "2026-05-22T00:00:00.000Z",
            "2024-11-18T00:00:00.000Z",
        ]
    );
    let upcoming: Vec<_> = requests
        .iter()
        .filter(|query| !query.contains_key("timeMax"))
        .collect();
    assert_eq!(upcoming.len(), 1);
    assert_eq!(upcoming[0]["timeMin"], "2026-11-18T00:00:00.000Z");
}

#[tokio::test]
async fn query_in_default_mode_keeps_past_then_upcoming_and_the_counts() {
    let harness = Harness::start_with(
        (1..=4)
            .flat_map(|day| {
                [
                    day_event(&format!("p{day}"), "standup", 10 + day, 1),
                    day_event(&format!("o{day}"), "other", 10 + day, 3),
                    day_event(&format!("u{day}"), "standup", 18 + day, 1),
                    day_event(&format!("x{day}"), "other", 18 + day, 3),
                ]
            })
            .collect(),
    )
    .await;
    let output = calendar_text(
        &harness,
        &["--query", "standup", "--past", "2", "--upcoming", "3"],
    )
    .await;
    let rows: Vec<(&str, &str)> = output
        .lines()
        .skip(1)
        .map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            (columns[0], columns[1])
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("p3", "past"),
            ("p4", "past"),
            ("u1", "upcoming"),
            ("u2", "upcoming"),
            ("u3", "upcoming"),
        ]
    );
    let output = calendar_text(
        &harness,
        &["--query", "standup", "--past", "3", "--upcoming", "0"],
    )
    .await;
    assert_eq!(ids(&output), ["p2", "p3", "p4"]);
    let skipped = harness.requests(EVENTS_PATH).await;
    assert!(
        skipped[2..]
            .iter()
            .all(|query| query.contains_key("timeMax"))
    );
    assert!(skipped.iter().all(|query| query["q"] == "standup"));
}
