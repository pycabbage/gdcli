# Architecture

The product description is the first line of README.md (also `description` in Cargo.toml and the `--help` text; a test keeps them identical). One library crate (`gdcli`, `src/lib.rs`) holds all logic; `src/main.rs` only parses arguments, runs the library on a single-threaded tokio runtime, prints `error: <message>` to stderr and exits with status 1 on failure (clap exits with 2 on usage errors; a closed output pipe exits 0 silently).

## Module layout

| Path | Responsibility |
|---|---|
| `src/main.rs` | `#[tokio::main(flavor = "current_thread")]`, error reporting |
| `src/lib.rs` | `run(Cli)`: resolves paths, dispatches commands (the paths are read through `Paths::resolve`, which tests drive with fake environments); `deadline_for(&Command)` (120 s for API commands, none for `setup` and `login` because they wait for the user); `is_broken_pipe()` (only a top-level `io::Error` of kind `BrokenPipe`); `message()` (the error chain on one line); `now()` (current time, whole seconds) |
| `src/cli.rs` | clap derive definitions (`ListArgs` with `--past`, `--upcoming`/`--future`, `--json`; `GetArgs`; `docs get --tab`; `docs metadata`); the `event_id` value parser rejects empty ids, `.`, `..` and ids containing `/ ? # %`, the `document_id` parser rejects empty, `.` and `..` ids, before any request |
| `src/paths.rs` | `Paths { config, data }`: XDG directories (absolute `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME`, else absolute `$HOME` or the passwd home directory plus `.config` / `.local/share`; relative values are ignored per the spec) and the two file names; `Paths::new` takes explicit directories (tests) |
| `src/google.rs` | the one REST layer and the HTTP plumbing: `Google` (hyper client, yup-oauth2 authenticator, the Calendar, Drive, Meet, Docs API and Docs web roots, and the predicate for the one allowed export redirect), `send()` / `json()` (one percent-encoded path segment per id, `.` and `..` rejected, bearer token for the three pinned scopes, JSON error message extraction cut at 300 characters, no body echo), `export_tab()` (Docs web export with one redirect to `*.googleusercontent.com` fetched without the token, content-type check), `ApiError`, the https-only rustls connector with the 10 s connect timeout, the HTTP/2-only client builder, `with_deadline()` |
| `src/events.rs` | the one list implementation for `calendar list` and `meet list`: `list()`, `upcoming()`, `past()` with window widening, the `eventTypes` filter, time helpers; the two header constants |
| `src/calendar.rs` | `calendar list` (calls `events::list`), `calendar get`: the curated record model of an event |
| `src/meet.rs` | `meet list` (calls `events::list`), `meet get <event id>`: `events.get`, then `spaces.get` and `conferenceRecords.list` concurrently, then participants, recordings and transcripts per record, assembled into one record; each Meet call tolerates 403/404 on its own |
| `src/docs.rs` | `docs get` (whole document through Drive export, or one tab through `export_tab`), tab list and tab resolution (`tabs()`, `resolve_tab()`), `docs metadata` |
| `src/output.rs` | `compact()` (drop empty values), `record()` (JSON line, or the generic text rendering of the same record), `table()` (header and TSV rows, or a JSON array of objects keyed by the header) |
| `src/auth/mod.rs` | `SCOPES` |
| `src/auth/token.rs` | `authenticator()` (login flow or fail-fast flow), `login()` |
| `src/auth/setup.rs` | `gdcli setup` wizard (embeds `docs/setup.md` with `include_str!`), client secret validation and installation (mode 0600) |

Command functions take `&Google`, the current time where needed, and a caller supplied `impl Write` for output. Production builds `Google` with `Google::connect`; tests build it with `Google::new` and a plain-HTTP connector against a local mock server, and a `yup_oauth2::AccessTokenAuthenticator` as token source. The only text printed behind the writer's back is the authorization URL line, which yup-oauth2's default flow delegate prints with `println!`.

Events, Meet resources and Drive answers are handled as `serde_json::Value`: the printed fields are read with JSON pointers, which is smaller than typed models for the few fields used (the earlier typed Meet models and the generated Calendar types cost more lines than they saved). Each `get` command turns the answer into one ordered record (`serde_json` with `preserve_order`, so key order is the contract) and `output::record` renders that record either as JSON or as text; there is no per-command text formatting.

## Data flow

1. `run` resolves `Paths` from the XDG variables and the optional deadline from the command.
2. API commands call `Google::connect`: read `client_secret.json`, build a non-interactive yup-oauth2 authenticator backed by `token.json`, build one hyper client (rustls, HTTP/2 only) shared by the API calls and the OAuth token requests.
3. The command issues its API calls (concurrently with `tokio::join!` when independent), converts the JSON to text and writes it to stdout.

## Dependencies

The list, the trust audit, the accepted risks and the exit plans are in `dependencies.md`. Why the Calendar, Drive and Meet clients are one small hand-written layer instead of generated crates is recorded there too.

## Known limitations

- Single user, single (primary) calendar.
- All-day events have no time zone in the API response; for the "has it ended" decision their end date is treated as midnight UTC.
- Timed events print in UTC (`Z`) everywhere; the Docs web export of `docs get --tab` relies on an undocumented `tab=` parameter (see `commands.md`).
- Meet REST API visibility rules apply: conference records, participants, recordings and transcripts are returned only as far as the API grants them to the authenticated user; gdcli does not work around them.
- `docs get` is limited by the Drive export size limit (10 MB).
- Participants are limited to one page of 250, recordings and transcripts to one page of 100.
