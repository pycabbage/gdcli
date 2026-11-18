use crate::auth::{self, SCOPES};
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use http::header::{AUTHORIZATION, CONTENT_TYPE, LOCATION};
use http::{Request, Response, StatusCode};
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

const MESSAGE_LIMIT: usize = 300;
const MARKDOWN_TYPES: [&str; 2] = ["text/markdown", "text/x-markdown"];
const ROOTS: [&str; 4] = [
    "https://www.googleapis.com/",
    "https://meet.googleapis.com/",
    "https://docs.googleapis.com/",
    "https://docs.google.com/",
];

pub fn googleusercontent(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".googleusercontent.com"))
}

pub struct Google {
    client: Client<Https, String>,
    auth: Authenticator<Https>,
    redirect_allowed: fn(&Url) -> bool,
    pub calendar: String,
    pub drive: String,
    pub meet: String,
    pub docs_api: String,
    docs_web: String,
}

impl Google {
    pub fn new(
        client: Client<Https, String>,
        auth: Authenticator<Https>,
        roots: [&str; 4],
        redirect_allowed: fn(&Url) -> bool,
    ) -> Self {
        let [google, meet, docs_api, docs_web] = roots;
        Self {
            client,
            auth,
            redirect_allowed,
            calendar: format!("{google}calendar/v3/"),
            drive: format!("{google}drive/v3/"),
            meet: format!("{meet}v2/"),
            docs_api: format!("{docs_api}v1/"),
            docs_web: docs_web.to_owned(),
        }
    }

    pub async fn connect(paths: &Paths) -> Result<Self> {
        let client = https_builder().build(connector());
        let auth = auth::authenticator(paths, false, client.clone()).await?;
        Ok(Self::new(client, auth, ROOTS, googleusercontent))
    }

    fn url(root: &str, segments: &[&str], query: &[(&str, &str)]) -> Result<Url> {
        let mut url = Url::parse(root)?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| anyhow!("invalid root url"))?;
            path.pop_if_empty();
            for segment in segments {
                anyhow::ensure!(!matches!(*segment, "." | ".."), "invalid id {segment:?}");
                path.push(segment);
            }
        }
        url.query_pairs_mut().extend_pairs(query);
        Ok(url)
    }

    async fn get(&self, url: &Url, bearer: bool) -> Result<Response<Incoming>> {
        let mut request = Request::get(url.as_str());
        if bearer {
            let token = self
                .auth
                .token(&SCOPES)
                .await
                .context("token retrieval failed")?;
            let token = token
                .token()
                .context("token retrieval failed: no access token")?;
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = self
            .client
            .request(request.body(String::new())?)
            .await
            .context("request failed")?;
        Ok(response)
    }

    async fn checked(response: Response<Incoming>) -> Result<Response<Incoming>> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.into_body().collect().await?.to_bytes();
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|body| Some(body.pointer("/error/message")?.as_str()?.to_owned()))
            .map(|message| match message.char_indices().nth(MESSAGE_LIMIT) {
                Some((end, _)) => format!("{}...", &message[..end]),
                None => message,
            });
        Err(ApiError { status, message }.into())
    }

    pub async fn send(
        &self,
        root: &str,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Response<Incoming>> {
        let url = Self::url(root, segments, query)?;
        Self::checked(self.get(&url, true).await?).await
    }

    pub async fn export_tab(&self, id: &str, tab: &str) -> Result<Response<Incoming>> {
        let query = [("format", "md"), ("tab", tab)];
        let url = Self::url(&self.docs_web, &["document", "d", id, "export"], &query)?;
        let mut response = self.get(&url, true).await?;
        if response.status().is_redirection() {
            let target = response
                .headers()
                .get(LOCATION)
                .and_then(|location| url.join(location.to_str().ok()?).ok())
                .filter(|target| (self.redirect_allowed)(target))
                .context("export redirected to an unexpected host")?;
            response = self.get(&target, false).await?;
        }
        let response = Self::checked(response).await?;
        let markdown = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|kind| kind.to_str().ok())
            .is_some_and(|kind| {
                MARKDOWN_TYPES
                    .iter()
                    .any(|accepted| kind.starts_with(accepted))
            });
        anyhow::ensure!(markdown, "unexpected export content type");
        Ok(response)
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
