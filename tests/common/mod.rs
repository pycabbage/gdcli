#![allow(dead_code)]

use chrono::{DateTime, Utc};
use gdcli::{Api, Endpoints};
use google_apis_common::Body;
use hyper_util::client::legacy::connect::HttpConnector;
use serde_json::{Value, json};
use std::collections::HashMap;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

pub const EVENTS_PATH: &str = "/calendar/v3/calendars/primary/events";
pub const TOKEN: &str = "test-token";
pub const BEARER: &str = "Bearer test-token";

pub struct Harness {
    pub server: MockServer,
    pub api: Api<HttpConnector>,
}

impl Harness {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let root = format!("{}/", server.uri());
        let client = gdcli::http::builder().build::<_, Body>(HttpConnector::new());
        let api = Api::new(
            client,
            TOKEN.to_owned(),
            &Endpoints {
                google: root.clone(),
                meet: root,
            },
        );
        Self { server, api }
    }

    pub async fn mount_json(&self, route: &str, body: Value) {
        Mock::given(method("GET"))
            .and(header("authorization", BEARER))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
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

pub fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value).unwrap().to_utc()
}

pub fn now() -> DateTime<Utc> {
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

fn moment(value: &Value) -> DateTime<Utc> {
    match value["dateTime"].as_str() {
        Some(date_time) => at(date_time),
        None => at(&format!("{}T00:00:00Z", value["date"].as_str().unwrap())),
    }
}

fn event_start(value: &Value) -> DateTime<Utc> {
    moment(&value["start"])
}

fn event_end(value: &Value) -> DateTime<Utc> {
    moment(&value["end"])
}

pub fn calendar_fake(events: Vec<Value>) -> impl Fn(&Request) -> ResponseTemplate {
    move |request| {
        let query: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let bound = |key: &str| query.get(key).map(|value| at(value));
        let time_min = bound("timeMin");
        let time_max = bound("timeMax");
        let mut hits: Vec<&Value> = events
            .iter()
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
        let end = (offset + size).min(hits.len());
        let next = (end < hits.len()).then(|| end.to_string());
        ResponseTemplate::new(200)
            .set_body_json(json!({"items": &hits[offset.min(end)..end], "nextPageToken": next}))
    }
}

pub fn text(buffer: Vec<u8>) -> String {
    String::from_utf8(buffer).unwrap()
}
