use crate::google::Https;
use anyhow::{Context, Result, anyhow, bail, ensure};
use http::header::{AUTHORIZATION, LOCATION, USER_AGENT};
use http::{HeaderValue, Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use ring::digest;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Seek, Write};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;
use yup_oauth2::hyper::body::Incoming;

const AGENT: &str = concat!("gdcli/", env!("CARGO_PKG_VERSION"));
const REDIRECTS: usize = 3;

pub fn github(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    url.scheme() == "https"
        && url.authority() == host
        && (matches!(host, "github.com" | "api.github.com")
            || host.ends_with(".githubusercontent.com"))
}

pub fn github_token(value: Option<OsString>) -> Option<String> {
    value
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
}

pub fn bearer(token: &str) -> Result<HeaderValue> {
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| anyhow!("GITHUB_TOKEN is not a valid HTTP header value"))?;
    value.set_sensitive(true);
    Ok(value)
}

pub struct Fetcher {
    pub client: Client<Https, String>,
    pub allowed: fn(&Url) -> bool,
}

impl Fetcher {
    pub(super) async fn send(
        &self,
        url: &str,
        mut credential: Option<&HeaderValue>,
    ) -> Result<(Response<Incoming>, bool)> {
        let mut url = Url::parse(url).context("invalid download url")?;
        let origin = url.origin();
        for _ in 0..=REDIRECTS {
            ensure!(
                (self.allowed)(&url),
                "refusing to fetch from unexpected host or scheme {}://{}",
                url.scheme(),
                url.host_str().unwrap_or_default()
            );
            credential = credential.filter(|_| url.origin() == origin);
            let mut request = Request::get(url.as_str()).header(USER_AGENT, AGENT);
            if let Some(value) = credential {
                request = request.header(AUTHORIZATION, value.clone());
            }
            let request = request.body(String::new())?;
            let response = self
                .client
                .request(request)
                .await
                .context("request failed")?;
            let next = response
                .headers()
                .get(LOCATION)
                .and_then(|location| url.join(location.to_str().ok()?).ok())
                .filter(|_| response.status().is_redirection());
            let Some(next) = next else {
                return Ok((response, credential.is_some()));
            };
            url = next;
        }
        bail!("too many redirects")
    }

    pub(super) fn ok(
        response: Response<Incoming>,
        authenticated: bool,
    ) -> Result<Response<Incoming>> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status == StatusCode::UNAUTHORIZED && authenticated {
            bail!("github rejected GITHUB_TOKEN (401): unset it or provide a valid token");
        }
        let header = |name| response.headers().get(name)?.to_str().ok();
        if matches!(
            status,
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
        ) && header("x-ratelimit-remaining") == Some("0")
        {
            let reset = header("x-ratelimit-reset")
                .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds.parse().ok()?).ok())
                .and_then(|time| time.format(&Rfc3339).ok())
                .map_or_default(|time| format!("; resets at {time}"));
            if authenticated {
                bail!("github api rate limit of the GITHUB_TOKEN exceeded{reset}");
            }
            bail!(
                "github api rate limit exceeded{reset}; set GITHUB_TOKEN to raise the limit of 60 requests per hour"
            );
        }
        bail!("github api error {status}")
    }

    pub async fn archive(&self, url: &str, want: &str, mut file: File, limit: u64) -> Result<File> {
        let mut body = Self::ok(self.send(url, None).await?.0, false)?.into_body();
        let mut hash = digest::Context::new(&digest::SHA256);
        let mut size = 0;
        while let Some(frame) = body.frame().await {
            if let Some(data) = frame.context("download failed")?.data_ref() {
                size += data.len() as u64;
                ensure!(size <= limit, "download exceeds {limit} bytes");
                hash.update(data);
                file.write_all(data).context("cannot write the download")?;
            }
        }
        let got = hash
            .finish()
            .as_ref()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        ensure!(got == want, "checksum mismatch: expected {want}, got {got}");
        file.rewind()?;
        Ok(file)
    }
}
