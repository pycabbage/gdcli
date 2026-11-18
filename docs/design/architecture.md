# Architecture

gdcli is a read-only Google Workspace CLI for AI agents. One library crate (`gdcli`, `src/lib.rs`) holds all logic; `src/main.rs` only parses arguments, runs the library on a single-threaded tokio runtime, prints `error: <message>` to stderr and exits with status 1 on failure (clap exits with 2 on usage errors).

## Module layout

| Path | Responsibility |
|---|---|
| `src/main.rs` | `#[tokio::main(flavor = "current_thread")]`, error reporting |
| `src/lib.rs` | `run(Cli)`: resolves paths, dispatches commands; `message()` formats an error chain on one trimmed string; `now()` is the current time truncated to whole seconds |
| `src/cli.rs` | clap derive definitions: `Cli`, `Command`, `MeetCommand`, `CalendarCommand`, `DocsCommand` |
| `src/paths.rs` | `Paths { config, data }`: XDG directories and the two file names; `Paths::new` takes explicit directories (tests), `Paths::from_xdg` uses the `xdg` crate |
| `src/http.rs` | rustls connector (ring provider, bundled webpki roots, https only, HTTP/1.1 and HTTP/2) and the hyper client builder |
| `src/api.rs` | `Api<C>`: `CalendarHub`, `DriveHub` and the hand-written `Meet` client sharing one hyper client and one token source; `Endpoints` holds the two root URLs (Google APIs, Meet API) so tests can point everything at a mock server; `Api::connect` builds the production instance |
| `src/auth/mod.rs` | `SCOPES` |
| `src/auth/token.rs` | `Token` (the `GetToken` implementation), `authenticator()`, `login()` |
| `src/auth/secret.rs` | client secret validation and installation (mode 0600) |
| `src/auth/setup.rs` | `gdcli setup` wizard; embeds `docs/setup.md` with `include_str!` |
| `src/calendar/events.rs` | shared Calendar queries: `upcoming()`, `past()` with window widening, time helpers |
| `src/calendar/list.rs`, `get.rs` | `calendar list`, `calendar get`, row and field rendering |
| `src/meet/client.rs` | minimal Meet REST v2 client (`Meet<C>`) |
| `src/meet/model.rs` | serde models of the Meet resources that are printed |
| `src/meet/list.rs`, `get.rs` | `meet list`, `meet get`, id parsing, rendering |
| `src/docs.rs` | `docs get`: document id extraction and streaming Markdown export |
| `src/output.rs` | `row()` (TSV line) and `field()` / `field_opt()` (`key: value` lines) |

Every command function (including `login` and `setup`) is generic over the hyper connector `C` where it calls an API and writes to a caller supplied `impl Write`; the only text printed behind the writer's back is the authorization URL line, which yup-oauth2's default flow delegate prints with `println!`; `now` is a parameter. Production uses `Api<Https>` and stdout; tests use `Api<HttpConnector>` against a local mock server and a `Vec<u8>`.

## Data flow

1. `run` resolves `Paths` from XDG variables.
2. Non-auth commands call `Api::connect`: read `client_secret.json`, build a non-interactive yup-oauth2 authenticator backed by `token.json`, build one `Client` with one TLS connector and share it between the three API front ends.
3. The command issues its API calls (concurrently with `tokio::try_join!` when independent), converts responses to text and writes them to stdout.

## Dependencies and why

| Crate | Why |
|---|---|
| `clap` (derive, no default features) | CLI parsing, `--help`, `--version`; only std, derive, help, usage, error-context features |
| `xdg` | XDG base directory resolution (including the "relative paths are ignored" rule) |
| `yup-oauth2` | OAuth installed-app flow with loopback redirect, token persistence (mode 0600) and refresh; default features off, `hyper-rustls` + `ring` on |
| `google-calendar3`, `google-drive3` | Generated clients from google-apis-rs for Calendar v3 and Drive v3; default features off (no second TLS stack feature set beyond ring) |
| `google-apis-common` | `GetToken`, `Client`, `Body`, shared by the generated crates and by the Meet client |
| `hyper-rustls` (ring, webpki-roots), `hyper-util` | HTTPS client; no OpenSSL, no system certificate scanning, cross-compiles with zig |
| `http`, `http-body-util`, `url` | request building, streaming of response bodies, URL building with percent-encoded path segments for the Meet client |
| `serde`, `serde_json` | Meet models and validation of client secrets |
| `chrono` (clock only) | time arithmetic and RFC 3339 formatting; it is the time type of the generated Calendar crate |
| `tokio` (rt, macros) | single-threaded runtime and `try_join!` |
| `anyhow` | error context |
| dev: `wiremock`, `tempfile` | local mock HTTP server and temporary directories for tests |

### Meet: hand-written client versus Google's generated SDK

The owner's rule is to write as little source as possible, so the Meet client was evaluated against every published library (searched on crates.io with `cargo search` and the crates.io API for `google meet`, `google-apps-meet`, `google-cloud-apps-meet`, `google-cloud-meet`, `google-meet2`, `meet v2`, `meet.googleapis.com`, `conferenceRecords` and others, plus the google-apis-rs API list, 290 entries, and the Google discovery directory, which lists `meet:v2`).

| Candidate | Maintainer, size | What it is | Verdict |
|---|---|---|---|
| `google-apps-meet-v2` 1.2.0 | Google LLC (googleapis/google-cloud-rust, published by `cloud-sdk-rust-bot`), Apache-2.0, MSRV 1.91, 51 downloads (first published 2026-07-13) | Generated REST client with full typed models for both services (`SpacesService`, `ConferenceRecordsService`); HTTP through reqwest, auth through `google-cloud-auth` | Works, measured below, rejected on the size and memory budget |
| `googleapis-tonic-google-apps-meet-v2` 0.30.0 and `-v2beta` 0.32.0 | bouzuya (community), about 15.5k and 16.9k downloads | tonic/prost gRPC stubs generated from the proto files; no REST, no auth, no endpoint handling | Not built: needs tonic, prost, h2 and its own TLS channel on top of the existing hyper stack, which is expected to be heavier than the REST SDK (not measured) |
| `rightkit-google-apis` 0.2.0 | bogusyogi, 55 downloads, first published 2026-11-02 | Gmail, Calendar, Drive, People, Tasks and Meet clients with its own desktop OAuth; the Meet module is 54 lines and only has `create_space` | Not usable: no conference records, participants, recordings or transcripts, and a second OAuth stack |
| `google-meet2` (google-apis-rs naming) | none | does not exist on crates.io; google-apis-rs has no Meet entry, so there is no crate of the style used for Calendar and Drive | Not available |

Measured with `google-apps-meet-v2` ported into a scratch copy of the repo (`default-features = false`, the crate's only feature is the aws-lc based `default-rustls-provider`; the direct dependencies `google-cloud-auth` (no default features), `google-cloud-wkt` and `rustls` with only the `ring` feature were added, so aws-lc-rs is not in the tree; same release profile, same machine, rustc 1.99):

| Measurement | Hand-written (current) | With `google-apps-meet-v2` | Delta |
|---|---|---|---|
| Stripped release binary | 4,330,368 bytes | 7,396,552 bytes | +3,066,184 bytes (+3.07 MB, +2.92 MiB) |
| Unique packages incl. gdcli (`cargo tree -e normal --prefix none --no-dedupe \| sed 's/ (.*//' \| sort -u \| wc -l`) | 123 | 183 | +60 |
| Peak RSS `gdcli --help` (3 runs, KiB) | 3580, 3644, 3636 | 4516, 4400, 4612 | about +0.9 MB |
| Peak RSS `meet list` with a real HTTPS round trip to Google and an invalid token (3 runs, KiB) | 6252, 6236, 6224 | 8896, 8888, 8808 | about +2.6 MB (+2627 KiB on the means) |
| Peak RSS `calendar list`, same method (KiB) | 6224, 6288, 6136 | 8640, 8736, 8564 | about +2.4 MB |
| Lines in `src/meet/` | 499 (`client.rs` 121, `model.rs` 80, `get.rs` 204, `list.rs` 67, `mod.rs` 27) | 458 (`client.rs` 67, `get.rs` 289, `list.rs` 76, `mod.rs` 26) | -41 |
| Clean release build | 41.5 s | 74.1 s (+32.6 s) | |

The SDK can be driven by the existing token and the endpoint is overridable. An about 40-line `CredentialsProvider` adapter (`Bearer`, part of the 67-line `client.rs` that also built the two service clients) that asks the existing `GetToken` for `SCOPES` and returns an `Authorization: Bearer` header works, `with_endpoint` points the clients at the wiremock server, and with the `fields` assertion removed all five Meet tests passed unchanged against the library's models (the 401 error text through `gdcli::message` was identical).

Decision rule and result:

| Criterion | Result |
|---|---|
| (a) authenticates with the existing yup-oauth2 token, no second login or token file | met |
| (b) endpoint overridable, wiremock tests still work | met |
| (c) binary grows by at most 3.0 MB and peak RSS of a real HTTPS call by at most 2.5 MB | not met: +3.07 MB (+2.92 MiB) binary and about +2.6 MB RSS; the RSS growth fails in both MB and MiB, and building only the one needed service client made no measurable difference (8704, 8900, 8860 KiB for `meet list`) |
| (d) removes the hand-written HTTP client and models | met for the client and the models, but the mapping from the library's models (enums with `*_UNSPECIFIED`, empty strings for absent values, `Timestamp`, oneof wrappers, per-call async wrappers for `try_join!`) grew `get.rs` and `list.rs`, so the net saving is 41 lines |

Not adopted because of (c). Further costs seen in the trial: the SDK offers no partial response (`fields`) parameter, so Meet responses would be larger than today; reqwest's rustls (no provider) pulls in `rustls-platform-verifier` and `rustls-native-certs` (system certificate store) instead of the bundled `webpki-roots`, and HTTPS to Google worked on the test machine only after installing the ring provider by hand; the clients retry transient errors by default (up to 10 attempts or 60 s); two more reqwest clients (one per service) with their own connection pools and TLS configuration exist next to the hyper client; `Api::new` becomes async because the SDK clients are built asynchronously; and the process-wide rustls provider must be installed explicitly. The remaining 85 crates are unconditional dependencies of the SDK transport (`google-cloud-gax-internal`: reqwest, `google-cloud-auth`, tower, OpenTelemetry and tracing crates, futures), so no feature trimming removes them.

The Meet client therefore stays hand-written (`src/meet/client.rs`, about 120 lines: one generic authenticated GET returning a deserialized model, plus six thin methods; `src/meet/model.rs`, about 80 lines of serde structs for the printed fields only), sharing the hyper client and token source of the Calendar and Drive hubs. Revisit when the SDK offers a slimmer transport or when google-apis-rs publishes a Meet crate.

## Known limitations

- Single user, single (primary) calendar.
- All-day events have no time zone in the API response; for the "has it ended" decision their end date is treated as midnight UTC.
- Timestamps from the Calendar API are normalised to UTC (`Z`) because the generated crate parses them into `chrono::DateTime<Utc>`; the event time zone name is printed separately by `calendar get`.
- Meet REST API visibility rules apply: conference records, participants, recordings and transcripts are returned only as far as the API grants them to the authenticated user; gdcli does not work around them.
- `docs get` is limited by the Drive export size limit (10 MB).
- Participants are limited to one page of 250, recordings and transcripts to one page of 100.
