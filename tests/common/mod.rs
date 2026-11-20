#![allow(dead_code)]

use gdcli::cli::{AccountArg, ListArgs};
use gdcli::google::Google;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::{Value, json};
use std::collections::HashMap;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use wiremock::matchers::{any, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use yup_oauth2::AccessTokenAuthenticator;

pub const EVENTS_PATH: &str = "/calendar/v3/calendars/primary/events";
pub const TOKEN: &str = "test-token";
pub const EVENT_TYPES: [&str; 4] = ["default", "focusTime", "outOfOffice", "fromGmail"];
pub const BEARER: &str = "Bearer test-token";
pub const CODE: &str = "abc-defg-hij";
pub const EVENT_FIELDS: &str = "id,summary,start,end,conferenceData(conferenceId,conferenceSolution(key(type))),attachments(title,mimeType,fileId)";
pub const RECORDS_FIELDS: &str = "conferenceRecords(name,startTime,endTime)";

fn local(url: &url::Url) -> bool {
    url.host_str() == Some("127.0.0.1")
}

pub fn args() -> ListArgs {
    ListArgs {
        past: 5,
        upcoming: 5,
        from: None,
        to: None,
        query: None,
        limit: None,
        json: false,
        account: AccountArg::default(),
    }
}

pub struct Harness {
    pub server: MockServer,
    pub api: Google,
}

pub async fn google_at(root: &str) -> Google {
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);
    let auth = AccessTokenAuthenticator::builder(TOKEN.to_owned())
        .build()
        .await
        .unwrap();
    Google::new(client, auth, [root; 4], local)
}

pub async fn unreachable() -> Google {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    google_at(&root).await
}

impl Harness {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let api = google_at(&format!("{}/", server.uri())).await;
        Self { server, api }
    }

    pub async fn start_with(events: Vec<Value>) -> Self {
        let harness = Self::start().await;
        harness.mount_calendar(events).await;
        harness
    }

    pub async fn fail_all(&self, status: u16, body: &str) {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&self.server)
            .await;
    }

    pub async fn mount_json(&self, route: &str, body: Value) {
        self.mount_status(route, 200, body).await;
    }

    pub async fn mount_status(&self, route: &str, status: u16, body: Value) {
        Mock::given(method("GET"))
            .and(header("authorization", BEARER))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    pub async fn mount_about(&self, email: &str) {
        self.mount_json("/drive/v3/about", json!({"user": {"emailAddress": email}}))
            .await;
    }

    pub async fn mount_doc(&self, id: &str, markdown: &str) {
        Mock::given(method("GET"))
            .and(header("authorization", BEARER))
            .and(path(format!("/drive/v3/files/{id}/export")))
            .and(query_param("mimeType", "text/markdown"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(markdown.to_owned(), "text/markdown"),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn mount_collection(&self, record: &str, kind: &str, items: Value) {
        self.mount_json(
            &format!("/v2/conferenceRecords/{record}/{kind}"),
            json!({ kind: items }),
        )
        .await;
    }

    pub async fn mount_records(&self, filter: &str, records: Value) {
        Mock::given(method("GET"))
            .and(header("authorization", BEARER))
            .and(path("/v2/conferenceRecords"))
            .and(query_param("filter", filter))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"conferenceRecords": records})),
            )
            .mount(&self.server)
            .await;
    }

    pub async fn mount_calendar(&self, events: Vec<Value>) {
        Mock::given(method("GET"))
            .and(header("authorization", BEARER))
            .and(path(EVENTS_PATH))
            .respond_with(calendar_fake(events))
            .mount(&self.server)
            .await;
    }

    pub async fn query_values(&self, route: &str, key: &str) -> Vec<Vec<String>> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == route)
            .map(|request| {
                request
                    .url
                    .query_pairs()
                    .filter(|(name, _)| name == key)
                    .map(|(_, value)| value.into_owned())
                    .collect()
            })
            .collect()
    }

    pub async fn requests(&self, route: &str) -> Vec<HashMap<String, String>> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == route)
            .map(|request| request.url.query_pairs().into_owned().collect())
            .collect()
    }
}

pub fn at(value: &str) -> OffsetDateTime {
    OffsetDateTime::parse(value, &Rfc3339).unwrap()
}

pub fn rfc3339(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap()
}

pub fn millis(value: OffsetDateTime) -> String {
    format!("{}.000Z", rfc3339(value).trim_end_matches('Z'))
}

pub fn now() -> OffsetDateTime {
    at("2026-11-18T00:00:00Z")
}

pub fn event(id: &str, start: &str, end: &str) -> Value {
    json!({
        "id": id,
        "status": "confirmed",
        "summary": format!("Event {id}"),
        "start": {"dateTime": start},
        "end": {"dateTime": end},
    })
}

pub fn titled(id: &str, summary: &str, start: &str, end: &str) -> Value {
    let mut value = event(id, start, end);
    value["summary"] = json!(summary);
    value
}

pub fn records_filter() -> &'static str {
    "space.meeting_code = \"abc-defg-hij\" AND start_time>=\"2026-11-17T00:00:00.000Z\" AND start_time<=\"2026-11-17T03:00:00.000Z\""
}

pub fn cancelled(id: &str, start: &str, end: &str) -> Value {
    let mut value = event(id, start, end);
    value["status"] = json!("cancelled");
    value
}

pub fn meet_event(id: &str, code: &str, start: &str, end: &str) -> Value {
    let mut value = event(id, start, end);
    value["conferenceData"] = json!({
        "conferenceId": code,
        "conferenceSolution": {"key": {"type": "hangoutsMeet"}},
    });
    value
}

fn moment(value: &Value) -> OffsetDateTime {
    match value["dateTime"].as_str() {
        Some(date_time) => at(date_time),
        None => at(&format!("{}T00:00:00Z", value["date"].as_str().unwrap())),
    }
}

fn event_start(value: &Value) -> OffsetDateTime {
    moment(&value["start"])
}

fn event_end(value: &Value) -> OffsetDateTime {
    moment(&value["end"])
}

pub fn calendar_fake(events: Vec<Value>) -> impl Fn(&Request) -> ResponseTemplate {
    move |request| {
        let query: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let bound = |key: &str| query.get(key).map(|value| at(value));
        let time_min = bound("timeMin");
        let time_max = bound("timeMax");
        let needle = query.get("q").map(|text| text.to_lowercase());
        let mut hits: Vec<&Value> = events
            .iter()
            .filter(|value| {
                needle.as_ref().is_none_or(|needle| {
                    value["summary"]
                        .as_str()
                        .is_some_and(|summary| summary.to_lowercase().contains(needle))
                })
            })
            .filter(|value| time_min.is_none_or(|min| event_end(value) > min))
            .filter(|value| time_max.is_none_or(|max| event_start(value) < max))
            .collect();
        hits.sort_by_key(|value| event_start(value));
        let offset: usize = query
            .get("pageToken")
            .map_or(0, |token| token.parse().unwrap());
        let size: usize = query
            .get("maxResults")
            .map_or(250, |size| size.parse().unwrap());
        if size == 0 {
            return ResponseTemplate::new(400).set_body_json(
                json!({"error": {"code": 400, "message": "maxResults must be positive"}}),
            );
        }
        let end = (offset + size).min(hits.len());
        let next = (end < hits.len()).then(|| end.to_string());
        ResponseTemplate::new(200)
            .set_body_json(json!({"items": &hits[offset.min(end)..end], "nextPageToken": next}))
    }
}

pub fn text(buffer: Vec<u8>) -> String {
    String::from_utf8(buffer).unwrap()
}

fn token(name: &str, value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let plain = !text.is_empty()
                && !text.contains(|c: char| c.is_whitespace() || c == '"' || c == '=');
            let shown = if plain {
                text.clone()
            } else {
                serde_json::to_string(text).unwrap()
            };
            Some(format!("{name}={shown}"))
        }
        Value::Bool(_) | Value::Number(_) => Some(format!("{name}={value}")),
        _ => None,
    }
}

fn tokens(value: &Value, found: &mut Vec<String>) {
    for (name, item) in value.as_object().into_iter().flatten() {
        match item {
            Value::Object(_) => found.extend(token(name, &Value::String(item.to_string()))),
            other => found.extend(token(name, other)),
        }
    }
}

fn expected(key: &str, value: &Value, lines: &mut Vec<(String, String)>) {
    let singular = key.strip_suffix('s').unwrap_or(key);
    match value {
        Value::Array(items) if items.is_empty() => {
            lines.push((singular.to_owned(), "none".to_owned()));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| expected(singular, item, lines)),
        Value::Object(map) => {
            let mut found = Vec::new();
            tokens(value, &mut found);
            if !found.is_empty() {
                lines.push((key.to_owned(), found.join(" ")));
            }
            for (name, item) in map.iter().filter(|(_, item)| item.is_array()) {
                expected(name, item, lines);
            }
        }
        Value::String(text) => {
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            lines.push((key.to_owned(), text.trim().replace('\n', "\n  ")));
        }
        Value::Null => {}
        other => lines.push((key.to_owned(), other.to_string())),
    }
}

fn logical_lines(text: &str) -> Vec<(String, String)> {
    let mut lines: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let (true, Some((_, value))) = (line.starts_with("  "), lines.last_mut()) {
            value.push('\n');
            value.push_str(line);
        } else {
            let (key, value) = line.split_once(": ").unwrap_or((line, ""));
            lines.push((key.to_owned(), value.to_owned()));
        }
    }
    lines
}

pub fn assert_same_information(text: &str, json: &str) {
    let record: Value = serde_json::from_str(json).unwrap();
    let mut lines = Vec::new();
    for (key, item) in record.as_object().unwrap() {
        expected(key, item, &mut lines);
    }
    assert_eq!(logical_lines(text), lines, "{text}");
}
