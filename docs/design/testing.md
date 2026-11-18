# Testing

`cargo test` needs no credentials and no network access. Everything under `tests/` uses the library API or the built binary.

| File | Covers |
|---|---|
| `tests/units.rs` | meeting-code detection, meet id parsing, document id and URL extraction, TSV and `key: value` output helpers, event time rendering, ended/cancelled classification, Meet link detection, path joining |
| `tests/cli.rs` | clap `debug_assert`, parsing of every command in the spec, rejection of unspecified flags (`docs get --json`, `--limit`, ...) and missing arguments, `--help`/`--version` |
| `tests/calendar_list.rs` | past/upcoming selection against a fake Calendar server (a closure that filters and pages a dataset by `timeMin`, `timeMax`, `maxResults`, `pageToken`): single window, window widening (asserts the exact `timeMin` of each request), fewer than 5 events in all windows, 300 events over two pages with the 5 newest kept, cancelled events paged over, ongoing events only in upcoming, all-day events, sanitised summaries, API error text |
| `tests/calendar_get.rs` | full `calendar get` rendering (time zone handling, multi-line description, attendees, conference data), partial-response `fields`, 404 |
| `tests/meet.rs` | `meet list` merge of conference records and upcoming Meet events (non-Meet, other conference types and cancelled events excluded, paging until 5 found), `meet get` by meeting code and by record id with and without prefix, error messages |
| `tests/docs.rs` | Markdown export streaming of a 1 MB body for a bare id and several URL shapes, export errors |
| `tests/secret.rs` | client secret validation (installed vs web vs garbage), copy with mode 0600, directory creation, tightening of an existing 0666 file, rejection without touching the destination, `setup` prompt behaviour for bad paths and empty answers |
| `tests/binary.rs` | the real binary with `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `HOME` set per process (no mutation of the test process environment): missing client secret, XDG and `$HOME` fallback paths, missing token fails fast with `gdcli login` for every API command, corrupt token file, usage error exit code 2, help and version |
| `tests/common/mod.rs` | shared mock server harness (`wiremock`), `Api` wired to the mock through `Endpoints`, event builders and the fake Calendar server |

Testability design: command functions take the API facade, the current time and the output writer as parameters; `Endpoints` redirects the three API roots; the token source is a plain `String` in tests (`GetToken` is implemented for `String`); `Paths::new` takes explicit directories.

Not covered by automated tests (needs real credentials or Google's servers): the interactive OAuth login against Google, token refresh, and real API payloads. During development the login URL and loopback server were exercised manually with a fake client id, and API calls with a bogus token (Google answered 401, proving TLS with the bundled roots and request routing for Calendar, Drive and Meet).

The local quality gates are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and `cargo build --release --locked`. CI (`.github/workflows/ci.yml`) only builds and tests.
