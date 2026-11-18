use crate::auth::{self, SCOPES};
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use http::{Request, Response, StatusCode, header::AUTHORIZATION};
use http_body_util::BodyExt;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::{Builder, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use serde_json::Value;
use std::fmt;
use std::future::Future;
use std::time::Duration;
use url::Url;
use yup_oauth2::authenticator::Authenticator;
use yup_oauth2::hyper::body::Incoming;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: Option<String>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.message {
            Some(message) => write!(f, "google api error {}: {message}", self.status.as_u16()),
            None => write!(f, "google api error {}", self.status),
        }
    }
}

impl std::error::Error for ApiError {}

pub struct Google {
    client: Client<Https, String>,
    auth: Authenticator<Https>,
    pub calendar: String,
    pub drive: String,
    pub meet: String,
}

impl Google {
    pub fn new(
        client: Client<Https, String>,
        auth: Authenticator<Https>,
        google_root: &str,
        meet_root: &str,
    ) -> Self {
        Self {
            client,
            auth,
            calendar: format!("{google_root}calendar/v3/"),
            drive: format!("{google_root}drive/v3/"),
            meet: format!("{meet_root}v2/"),
        }
    }

    pub async fn connect(paths: &Paths) -> Result<Self> {
        let client = https_builder().build(connector());
        let auth = auth::authenticator(paths, false, client.clone()).await?;
        Ok(Self::new(
            client,
            auth,
            "https://www.googleapis.com/",
            "https://meet.googleapis.com/",
        ))
    }

    pub async fn send(
        &self,
        root: &str,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Response<Incoming>> {
        let mut url = Url::parse(root)?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| anyhow::anyhow!("invalid root url"))?;
            path.pop_if_empty();
            for segment in segments {
                anyhow::ensure!(!matches!(*segment, "." | ".."), "invalid id {segment:?}");
                path.push(segment);
            }
        }
        url.query_pairs_mut().extend_pairs(query);
        let token = self
            .auth
            .token(&SCOPES)
            .await
            .context("token retrieval failed")?;
        let token = token
            .token()
            .context("token retrieval failed: no access token")?;
        let request = Request::get(url.as_str())
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(String::new())?;
        let response = self
            .client
            .request(request)
            .await
            .context("request failed")?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.into_body().collect().await?.to_bytes();
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|body| Some(body.pointer("/error/message")?.as_str()?.to_owned()));
        Err(ApiError { status, message }.into())
    }

    pub async fn json(
        &self,
        root: &str,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Value> {
        let body = self
            .send(root, segments, query)
            .await?
            .into_body()
            .collect()
            .await?;
        serde_json::from_slice(&body.to_bytes()).context("unexpected google api response")
    }
}

pub type Https = HttpsConnector<HttpConnector>;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEADLINE: Duration = Duration::from_secs(120);

pub fn connector() -> Https {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    http.set_connect_timeout(Some(CONNECT_TIMEOUT));
    HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .enable_http2()
        .wrap_connector(http)
}

pub fn https_builder() -> Builder {
    let mut builder = Client::builder(TokioExecutor::new());
    builder.http2_only(true);
    builder
}

pub async fn with_deadline<T>(
    deadline: Option<Duration>,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    let Some(deadline) = deadline else {
        return future.await;
    };
    tokio::time::timeout(deadline, future)
        .await
        .map_err(|_| anyhow!("timed out after {}s", deadline.as_secs()))?
}
