use flate2::Compression;
use flate2::write::GzEncoder;
use gdcli::update::{Fetcher, Install, github, run_with};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use ring::digest;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LIMIT: u64 = 1 << 20;
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const NO_PUBLISHED_RELEASE: &str = "github has no published release for pycabbage/gdcli";
const BINARY: &str = "gdcli";
const TOKEN: &str = "ghp_testtoken";

fn local(url: &Url) -> bool {
    github(url) || url.host_str() == Some("127.0.0.1")
}

fn keep(staged: &Path) -> io::Result<()> {
    std::fs::copy(staged, staged.parent().unwrap().join("installed")).map(|_| ())
}

fn deny(_: &Path) -> io::Result<()> {
    Err(io::ErrorKind::PermissionDenied.into())
}

fn sha256(bytes: &[u8]) -> String {
    digest::digest(&digest::SHA256, bytes)
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

fn archive(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, kind, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_entry_type(*kind);
        header.set_cksum();
        tar.append(&header, *data).unwrap();
    }
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&tar.into_inner().unwrap()).unwrap();
    gzip.finish().unwrap()
}

fn regular(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<_> = entries
        .iter()
        .map(|(name, data)| (*name, tar::EntryType::Regular, *data))
        .collect();
    archive(&entries)
}

fn asset_name() -> String {
    format!(
        "gdcli_{}_{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn message(result: anyhow::Result<()>) -> String {
    format!("{:#}", result.unwrap_err())
}

fn hidden(text: &str) {
    assert!(!text.contains(TOKEN), "{text}");
}

struct Fixture {
    server: MockServer,
    root: TempDir,
    fetcher: Fetcher,
}

impl Fixture {
    async fn new() -> Self {
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("bin")).unwrap();
        Self {
            server: MockServer::start().await,
            root,
            fetcher: Fetcher {
                client: Client::builder(TokioExecutor::new()).build(connector),
                allowed: local,
            },
        }
    }

    async fn serving(bytes: &[u8]) -> Self {
        let fixture = Self::new().await;
        fixture
            .latest(fixture.release("v9.9.9", &sha256(bytes)))
            .await;
        fixture
            .mount(
                "/asset",
                ResponseTemplate::new(200)
                    .set_body_bytes(bytes.to_vec())
                    .insert_header("location", fixture.url("/other").as_str()),
            )
            .await;
        fixture
    }

    fn dir(&self) -> PathBuf {
        self.root.path().join("bin")
    }

    fn url(&self, route: &str) -> String {
        format!("{}{route}", self.server.uri())
    }

    fn release(&self, tag: &str, digest: &str) -> Value {
        json!({
            "tag_name": tag,
            "immutable": true,
            "assets": [
                {
                    "name": "gdcli_checksums.txt",
                    "browser_download_url": self.url("/checksums"),
                    "digest": "sha256:00",
                },
                {
                    "name": asset_name(),
                    "browser_download_url": self.url("/asset"),
                    "digest": format!("sha256:{digest}"),
                },
            ],
        })
    }

    async fn mount(&self, route: &str, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(template)
            .mount(&self.server)
            .await;
    }

    async fn latest(&self, release: Value) {
        self.mount("/latest", ResponseTemplate::new(200).set_body_json(release))
            .await;
    }

    async fn run_at(
        &self,
        api: &str,
        token: Option<&str>,
        limit: u64,
        replace: fn(&Path) -> io::Result<()>,
    ) -> (anyhow::Result<()>, String) {
        let install = Install {
            dir: self.dir(),
            replace,
            limit,
        };
        let mut out = Vec::new();
        let result = run_with(&self.fetcher, api, token, &install, &mut out).await;
        (result, String::from_utf8(out).unwrap())
    }

    async fn run_as(
        &self,
        token: Option<&str>,
        limit: u64,
        replace: fn(&Path) -> io::Result<()>,
    ) -> (anyhow::Result<()>, String) {
        self.run_at(&self.url("/latest"), token, limit, replace)
            .await
    }

    async fn run(
        &self,
        limit: u64,
        replace: fn(&Path) -> io::Result<()>,
    ) -> (anyhow::Result<()>, String) {
        self.run_as(None, limit, replace).await
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    fn installed(&self) -> Vec<u8> {
        std::fs::read(self.dir().join("installed")).unwrap()
    }

    async fn paths(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect()
    }
}

fn nothing() -> Vec<String> {
    Vec::new()
}

#[tokio::test]
async fn a_newer_release_is_staged_verified_and_handed_to_the_replace_function() {
    let fixture = Fixture::serving(&regular(&[("README", b"x"), (BINARY, b"new binary")])).await;
    let (result, out) = fixture.run(LIMIT, keep).await;
    result.unwrap();
    assert_eq!(
        out,
        format!("status: updated\ncurrent: {CURRENT}\nlatest: 9.9.9\n")
    );
    assert_eq!(fixture.installed(), b"new binary");
    assert_eq!(fixture.files(), ["installed"]);
    assert_eq!(fixture.paths().await, ["/latest", "/asset"]);
    for request in fixture.server.received_requests().await.unwrap() {
        assert!(request.headers.get("authorization").is_none());
        assert_eq!(
            request.headers.get("user-agent").unwrap(),
            format!("gdcli/{CURRENT}").as_str()
        );
    }
}

#[tokio::test]
async fn equal_and_older_releases_are_up_to_date_and_never_download() {
    for tag in [format!("v{CURRENT}"), "v0.0.1".to_owned()] {
        let fixture = Fixture::new().await;
        fixture.latest(fixture.release(&tag, "00")).await;
        let (result, out) = fixture.run(LIMIT, keep).await;
        result.unwrap();
        let latest = tag.trim_start_matches('v');
        assert_eq!(
            out,
            format!("status: up_to_date\ncurrent: {CURRENT}\nlatest: {latest}\n")
        );
        assert_eq!(fixture.files(), nothing());
        assert_eq!(fixture.paths().await, ["/latest"]);
    }
}

#[tokio::test]
async fn a_refused_release_downloads_nothing_and_creates_nothing() {
    let without_digest = json!([{
        "name": asset_name(),
        "browser_download_url": "https://github.com/x/gdcli",
    }]);
    for (key, value) in [
        ("tag_name", json!("v99.0.0-rc.1")),
        ("tag_name", json!("99.0.0")),
        ("immutable", json!(false)),
        ("assets", without_digest),
        ("assets", json!([])),
    ] {
        let fixture = Fixture::new().await;
        let mut release = fixture.release("v99.0.0", "00");
        release[key] = value;
        fixture.latest(release).await;
        let (result, out) = fixture.run(LIMIT, keep).await;
        assert!(result.is_err(), "{key}");
        assert_eq!(out, "");
        assert_eq!(fixture.paths().await, ["/latest"]);
        assert_eq!(fixture.files(), nothing());
    }
    let fixture = Fixture::new().await;
    let mut release = fixture.release("v99.0.0", "00");
    release["assets"][1]["browser_download_url"] = json!("not a url");
    fixture.latest(release).await;
    let (result, _) = fixture.run(LIMIT, keep).await;
    assert!(message(result).starts_with("invalid download url: "));
    assert_eq!(fixture.paths().await, ["/latest"]);
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_digest_mismatch_fails_before_the_archive_is_opened_and_leaves_the_directory_empty() {
    let bytes = regular(&[(BINARY, b"x")]);
    let fixture = Fixture::new().await;
    fixture
        .latest(fixture.release("v9.9.9", &sha256(b"other")))
        .await;
    fixture
        .mount("/asset", ResponseTemplate::new(200).set_body_bytes(bytes))
        .await;
    let (result, out) = fixture.run(LIMIT, keep).await;
    let error = message(result);
    assert!(
        error.starts_with(&format!(
            "checksum mismatch: expected {}, got ",
            sha256(b"other")
        )),
        "{error}"
    );
    assert_eq!(out, "");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_rate_limited_answer_names_the_limit_the_reset_time_and_the_token_state() {
    let reset = ("x-ratelimit-reset", "1795052209");
    let limited = ("x-ratelimit-remaining", "0");
    let anonymous = "github api rate limit exceeded";
    let raise = "set GITHUB_TOKEN to raise the limit of 60 requests per hour";
    let authenticated = "github api rate limit of the GITHUB_TOKEN exceeded";
    for (status, headers, token, expected) in [
        (
            403,
            vec![limited, reset],
            None,
            format!("{anonymous}; resets at 2026-11-19T01:36:49Z; {raise}"),
        ),
        (
            429,
            vec![limited, reset],
            None,
            format!("{anonymous}; resets at 2026-11-19T01:36:49Z; {raise}"),
        ),
        (403, vec![limited], None, format!("{anonymous}; {raise}")),
        (
            403,
            vec![limited, ("x-ratelimit-reset", "soon")],
            None,
            format!("{anonymous}; {raise}"),
        ),
        (
            403,
            vec![limited, reset],
            Some(TOKEN),
            format!("{authenticated}; resets at 2026-11-19T01:36:49Z"),
        ),
        (
            429,
            vec![limited, reset],
            Some(TOKEN),
            format!("{authenticated}; resets at 2026-11-19T01:36:49Z"),
        ),
        (403, vec![limited], Some(TOKEN), authenticated.to_owned()),
    ] {
        let fixture = Fixture::new().await;
        let mut template = ResponseTemplate::new(status);
        for (name, value) in headers {
            template = template.insert_header(name, value);
        }
        fixture.mount("/latest", template).await;
        let (result, out) = fixture.run_as(token, LIMIT, keep).await;
        let error = message(result);
        assert_eq!(error, expected);
        hidden(&error);
        assert_eq!(out, "");
    }
}

#[tokio::test]
async fn statuses_map_to_specific_errors() {
    for (status, headers, expected) in [
        (404, vec![], NO_PUBLISHED_RELEASE),
        (500, vec![], "github api error 500 Internal Server Error"),
        (403, vec![], "github api error 403 Forbidden"),
        (
            403,
            vec![("x-ratelimit-remaining", "7")],
            "github api error 403 Forbidden",
        ),
    ] {
        let fixture = Fixture::new().await;
        let mut template = ResponseTemplate::new(status);
        for (name, value) in headers {
            template = template.insert_header(name, value);
        }
        fixture.mount("/latest", template).await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        assert_eq!(message(result), expected, "{status}");
    }
    for (template, expected) in [
        (ResponseTemplate::new(404), "github api error 404 Not Found"),
        (ResponseTemplate::new(302), "github api error 302 Found"),
        (
            ResponseTemplate::new(401),
            "github api error 401 Unauthorized",
        ),
    ] {
        let fixture = Fixture::new().await;
        fixture.latest(fixture.release("v9.9.9", "00")).await;
        fixture.mount("/asset", template).await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        assert_eq!(message(result), expected);
        assert_eq!(fixture.files(), nothing());
    }
}

#[tokio::test]
async fn a_body_that_is_not_json_is_reported() {
    for body in ["not json", ""] {
        let fixture = Fixture::new().await;
        fixture
            .mount("/latest", ResponseTemplate::new(200).set_body_string(body))
            .await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        let error = message(result);
        assert!(
            error.starts_with("unexpected github api response"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn a_redirect_to_an_allowed_host_is_followed() {
    for relative in [false, true] {
        let bytes = regular(&[(BINARY, b"moved")]);
        let fixture = Fixture::new().await;
        fixture
            .latest(fixture.release("v9.9.9", &sha256(&bytes)))
            .await;
        let location = if relative {
            "/real".to_owned()
        } else {
            fixture.url("/real")
        };
        fixture
            .mount(
                "/asset",
                ResponseTemplate::new(302).insert_header("location", location.as_str()),
            )
            .await;
        fixture
            .mount("/real", ResponseTemplate::new(200).set_body_bytes(bytes))
            .await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        result.unwrap();
        assert_eq!(fixture.installed(), b"moved");
        assert_eq!(fixture.paths().await, ["/latest", "/asset", "/real"]);
    }
}

#[tokio::test]
async fn a_redirect_to_a_foreign_host_or_scheme_is_refused_without_printing_secrets() {
    for (location, expected) in [
        (
            "https://user:pw@example.com:8443/a.tar.gz?sig=secret",
            "refusing to fetch from unexpected host or scheme https://example.com",
        ),
        (
            "http://github.com/a.tar.gz",
            "refusing to fetch from unexpected host or scheme http://github.com",
        ),
    ] {
        let fixture = Fixture::new().await;
        fixture.latest(fixture.release("v9.9.9", "00")).await;
        fixture
            .mount(
                "/asset",
                ResponseTemplate::new(302).insert_header("location", location),
            )
            .await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        assert_eq!(message(result), expected);
        assert_eq!(fixture.paths().await, ["/latest", "/asset"]);
        assert_eq!(fixture.files(), nothing());
    }
}

#[tokio::test]
async fn more_than_three_redirects_are_refused() {
    let fixture = Fixture::new().await;
    fixture.latest(fixture.release("v9.9.9", "00")).await;
    fixture
        .mount(
            "/asset",
            ResponseTemplate::new(302).insert_header("location", fixture.url("/asset").as_str()),
        )
        .await;
    let (result, _) = fixture.run(LIMIT, keep).await;
    assert_eq!(message(result), "too many redirects");
    let asset_requests = fixture
        .paths()
        .await
        .iter()
        .filter(|route| *route == "/asset")
        .count();
    assert_eq!(asset_requests, 4);
}

#[tokio::test]
async fn an_unreachable_api_is_reported_as_a_request_failure() {
    let fixture = Fixture::new().await;
    let (result, out) = fixture
        .run_at("http://127.0.0.1:1/latest", None, LIMIT, keep)
        .await;
    let error = message(result);
    assert!(error.starts_with("request failed"), "{error}");
    assert_eq!(out, "");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn an_archive_without_a_gdcli_entry_is_refused() {
    let fixture = Fixture::serving(&regular(&[("README", b"x"), ("bin/gdcli", b"y")])).await;
    let (result, _) = fixture.run(LIMIT, keep).await;
    assert_eq!(message(result), "archive has no regular file named gdcli");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_damaged_archive_with_a_matching_digest_is_refused() {
    let valid = regular(&[(BINARY, b"x")]);
    let truncated = valid[..valid.len() / 2].to_vec();
    let noise: Vec<u8> = (0_u32..128)
        .flat_map(|index| {
            digest::digest(&digest::SHA256, &index.to_le_bytes())
                .as_ref()
                .to_vec()
        })
        .collect();
    let large = regular(&[(BINARY, &noise)]);
    let cut_inside_the_entry = large[..large.len() * 3 / 4].to_vec();
    for bytes in [
        b"this is not gzip".to_vec(),
        truncated,
        cut_inside_the_entry,
    ] {
        let fixture = Fixture::serving(&bytes).await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        let error = message(result);
        assert!(error.starts_with("cannot extract the archive"), "{error}");
        assert_eq!(fixture.files(), nothing());
    }
}

#[tokio::test]
async fn only_the_entry_named_gdcli_is_installed() {
    let fixture = Fixture::serving(&regular(&[
        ("../x", b"1"),
        ("/abs", b"2"),
        ("x/gdcli", b"3"),
        ("./gdcli", b"4"),
        ("README", b"5"),
        (BINARY, b"good"),
    ]))
    .await;
    let (result, _) = fixture.run(LIMIT, keep).await;
    result.unwrap();
    assert_eq!(fixture.installed(), b"good");
    assert_eq!(fixture.files(), ["installed"]);
    let siblings: Vec<_> = std::fs::read_dir(fixture.root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(siblings, ["bin"]);
}

#[tokio::test]
async fn a_gdcli_entry_that_is_not_a_regular_file_is_refused() {
    for kind in [tar::EntryType::Symlink, tar::EntryType::Directory] {
        let fixture = Fixture::serving(&archive(&[(BINARY, kind, b"")])).await;
        let (result, _) = fixture.run(LIMIT, keep).await;
        assert_eq!(message(result), "archive has no regular file named gdcli");
        assert_eq!(fixture.files(), nothing());
    }
}

#[tokio::test]
async fn a_download_over_the_limit_is_refused() {
    let fixture = Fixture::new().await;
    fixture.latest(fixture.release("v9.9.9", "00")).await;
    fixture
        .mount(
            "/asset",
            ResponseTemplate::new(200).set_body_bytes(vec![7_u8; 4096]),
        )
        .await;
    let (result, _) = fixture.run(1024, keep).await;
    assert_eq!(message(result), "download exceeds 1024 bytes");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn an_entry_over_the_limit_is_refused() {
    let fixture = Fixture::serving(&regular(&[(BINARY, &vec![0_u8; 8192])])).await;
    let (result, _) = fixture.run(4096, keep).await;
    assert_eq!(
        message(result),
        "archive entry gdcli is larger than 4096 bytes"
    );
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn an_inflated_stream_over_four_times_the_limit_is_refused() {
    let fixture =
        Fixture::serving(&regular(&[("big", &vec![0_u8; 64 * 1024]), (BINARY, b"x")])).await;
    let (result, _) = fixture.run(4096, keep).await;
    let error = message(result);
    assert!(error.starts_with("cannot extract the archive"), "{error}");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn an_unwritable_install_directory_fails_before_the_download() {
    let fixture = Fixture::serving(&regular(&[(BINARY, b"x")])).await;
    let dir = fixture.dir();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let writable = std::fs::File::create(dir.join("probe")).is_ok();
    if writable {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: running as root");
        return;
    }
    let (result, _) = fixture.run(LIMIT, keep).await;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = message(result);
    assert!(
        error.starts_with(&format!("cannot write to {}: ", dir.display())),
        "{error}"
    );
    assert_eq!(fixture.paths().await, ["/latest"]);
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_failing_replace_function_is_reported_with_the_directory_and_cleans_up() {
    let fixture = Fixture::serving(&regular(&[(BINARY, b"x")])).await;
    let (result, out) = fixture.run(LIMIT, deny).await;
    assert_eq!(
        message(result),
        format!(
            "cannot replace the executable in {}: permission denied",
            fixture.dir().display()
        )
    );
    assert_eq!(out, "");
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_failed_write_of_the_download_is_reported_as_such() {
    let bytes = regular(&[(BINARY, b"x")]);
    let fixture = Fixture::serving(&bytes).await;
    let read_only = std::fs::File::open("/dev/null").unwrap();
    let error = fixture
        .fetcher
        .archive(&fixture.url("/asset"), &sha256(&bytes), read_only, LIMIT)
        .await
        .unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "cannot write the download: Bad file descriptor (os error 9)"
    );
}

#[tokio::test]
async fn a_body_that_breaks_midway_is_reported_as_a_download_failure() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut buffer);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\nshort");
    });
    let fixture = Fixture::new().await;
    let mut release = fixture.release("v9.9.9", "00");
    release["assets"][1]["browser_download_url"] = json!(format!("http://{address}/asset"));
    fixture.latest(release).await;
    let (result, _) = fixture.run(LIMIT, keep).await;
    let error = message(result);
    assert!(error.starts_with("download failed: "), "{error}");
    assert_eq!(fixture.files(), nothing());
}

async fn authorization_of(server: &MockServer) -> Vec<(String, Option<String>)> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            (
                request.url.path().to_owned(),
                request
                    .headers
                    .get("authorization")
                    .map(|value| value.to_str().unwrap().to_owned()),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_token_is_sent_to_the_api_request_only() {
    let fixture = Fixture::serving(&regular(&[(BINARY, b"new binary")])).await;
    let (result, out) = fixture.run_as(Some(TOKEN), LIMIT, keep).await;
    result.unwrap();
    hidden(&out);
    assert_eq!(fixture.installed(), b"new binary");
    assert_eq!(
        authorization_of(&fixture.server).await,
        [
            ("/latest".to_owned(), Some(format!("Bearer {TOKEN}"))),
            ("/asset".to_owned(), None),
        ]
    );
    for request in fixture.server.received_requests().await.unwrap() {
        assert_eq!(
            request.headers.get("user-agent").unwrap(),
            format!("gdcli/{CURRENT}").as_str()
        );
    }
}

#[tokio::test]
async fn a_redirect_of_the_api_request_to_another_authority_drops_the_token() {
    let other = MockServer::start().await;
    let fixture = Fixture::new().await;
    let target = format!("{}/elsewhere", other.uri());
    fixture
        .mount(
            "/latest",
            ResponseTemplate::new(302).insert_header("location", target.as_str()),
        )
        .await;
    Mock::given(method("GET"))
        .and(path("/elsewhere"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture.release("v0.0.1", "00")))
        .mount(&other)
        .await;
    let (result, out) = fixture.run_as(Some(TOKEN), LIMIT, keep).await;
    result.unwrap();
    assert!(out.starts_with("status: up_to_date"), "{out}");
    assert_eq!(
        authorization_of(&fixture.server).await,
        [("/latest".to_owned(), Some(format!("Bearer {TOKEN}")))]
    );
    assert_eq!(
        authorization_of(&other).await,
        [("/elsewhere".to_owned(), None)]
    );
}

#[tokio::test]
async fn a_redirect_of_the_api_request_within_the_same_authority_keeps_the_token() {
    let fixture = Fixture::new().await;
    fixture
        .mount(
            "/latest",
            ResponseTemplate::new(302).insert_header("location", "/moved"),
        )
        .await;
    fixture
        .mount(
            "/moved",
            ResponseTemplate::new(200).set_body_json(fixture.release("v0.0.1", "00")),
        )
        .await;
    let (result, out) = fixture.run_as(Some(TOKEN), LIMIT, keep).await;
    result.unwrap();
    assert!(out.starts_with("status: up_to_date"), "{out}");
    let bearer = Some(format!("Bearer {TOKEN}"));
    assert_eq!(
        authorization_of(&fixture.server).await,
        [
            ("/latest".to_owned(), bearer.clone()),
            ("/moved".to_owned(), bearer),
        ]
    );
}

#[tokio::test]
async fn a_token_that_is_not_a_valid_header_value_is_refused_before_any_request() {
    let bad = format!("{TOKEN}\nsecond line");
    let fixture = Fixture::serving(&regular(&[(BINARY, b"x")])).await;
    let (result, out) = fixture.run_as(Some(&bad), LIMIT, keep).await;
    let error = message(result);
    assert_eq!(error, "GITHUB_TOKEN is not a valid HTTP header value");
    hidden(&error);
    assert_eq!(out, "");
    assert_eq!(fixture.paths().await, nothing());
    assert_eq!(fixture.files(), nothing());
}

#[tokio::test]
async fn a_401_answer_names_the_token_variable_without_echoing_the_token() {
    for (token, expected) in [
        (
            Some(TOKEN),
            "github rejected GITHUB_TOKEN (401): unset it or provide a valid token",
        ),
        (None, "github api error 401 Unauthorized"),
    ] {
        let fixture = Fixture::new().await;
        fixture.mount("/latest", ResponseTemplate::new(401)).await;
        let (result, out) = fixture.run_as(token, LIMIT, keep).await;
        let error = message(result);
        assert_eq!(error, expected);
        hidden(&error);
        assert_eq!(out, "");
        assert_eq!(fixture.paths().await, ["/latest"]);
    }
}

#[tokio::test]
async fn a_401_from_another_authority_after_a_redirect_does_not_blame_the_token() {
    let other = MockServer::start().await;
    let fixture = Fixture::new().await;
    let target = format!("{}/elsewhere", other.uri());
    fixture
        .mount(
            "/latest",
            ResponseTemplate::new(302).insert_header("location", target.as_str()),
        )
        .await;
    Mock::given(method("GET"))
        .and(path("/elsewhere"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&other)
        .await;
    let (result, out) = fixture.run_as(Some(TOKEN), LIMIT, keep).await;
    let error = message(result);
    assert_eq!(error, "github api error 401 Unauthorized");
    hidden(&error);
    assert_eq!(out, "");
    assert_eq!(
        authorization_of(&other).await,
        [("/elsewhere".to_owned(), None)]
    );
}

#[tokio::test]
async fn a_hop_back_to_the_api_origin_does_not_bring_the_token_back() {
    let other = MockServer::start().await;
    let fixture = Fixture::new().await;
    let away = format!("{}/hop", other.uri());
    fixture
        .mount(
            "/latest",
            ResponseTemplate::new(302).insert_header("location", away.as_str()),
        )
        .await;
    let back = fixture.url("/final");
    Mock::given(method("GET"))
        .and(path("/hop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", back.as_str()))
        .mount(&other)
        .await;
    fixture
        .mount(
            "/final",
            ResponseTemplate::new(200).set_body_json(fixture.release("v0.0.1", "00")),
        )
        .await;
    let (result, out) = fixture.run_as(Some(TOKEN), LIMIT, keep).await;
    result.unwrap();
    assert!(out.starts_with("status: up_to_date"), "{out}");
    assert_eq!(
        authorization_of(&fixture.server).await,
        [
            ("/latest".to_owned(), Some(format!("Bearer {TOKEN}"))),
            ("/final".to_owned(), None),
        ]
    );
    assert_eq!(authorization_of(&other).await, [("/hop".to_owned(), None)]);
}
