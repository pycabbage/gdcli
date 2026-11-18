#![allow(dead_code)]

use gdcli::google::Google;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::{Value, json};
use std::collections::HashMap;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use yup_oauth2::AccessTokenAuthenticator;

pub const EVENTS_PATH: &str = "/calendar/v3/calendars/primary/events";
pub const TOKEN: &str = "test-token";
pub const EVENT_TYPES: [&str; 4] = ["default", "focusTime", "outOfOffice", "fromGmail"];
pub const BEARER: &str = "Bearer test-token";

pub struct Harness {
    pub server: MockServer,
    pub api: Google,
}

impl Harness {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let root = format!("{}/", server.uri());
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
        let api = Google::new(client, auth, &root, &root);
        Self { server, api }
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
