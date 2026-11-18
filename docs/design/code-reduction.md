# Code reduction log

Metric: non-blank lines after `cargo fmt` (`cat <files> | grep -vc '^\s*$'`), no comments exist anywhere. Baseline at the start of this wave (the tree that passed review): `src/` 1282, `tests/` 1928. Now: `src/` 929 (-353, -27.5 %), `tests/` 2042 (+114, because 12 tests were added for the new behaviour and 8 removed with the code they covered (2 of each are renames; net 75 to 79 tests); every earlier assertion on requests, paths, queries, `fields`, `eventTypes`, filters and `Authorization` was kept).

Per file, before to after (`src/`): `api.rs` 54 to 0, `http.rs` 36 to 0 (moved into `google.rs`), `listing.rs` 57 to 0 (into `events.rs`), `calendar/{events,get,list,mod}.rs` 265 to `events.rs` 172 + `calendar.rs` 17, `meet/{client,get,list,model,mod}.rs` 439 to `google.rs` 151 (HTTP plumbing and REST layer for all three APIs) + `meet.rs` 199, `auth/{mod,secret,setup,token}.rs` 187 to 159, `output.rs` 18 to 37, `paths.rs` 39 to 44, `lib.rs` 87 to 55, `cli.rs` 49 to 55, `docs.rs` 37 to 26.

| # | Experiment | Delta measured | Result |
|---|---|---|---|
| 1 | Replace google-calendar3, google-drive3, google-apis-common and chrono by one REST layer (`Google::json` / `send`) on the existing hyper stack, `serde_json::Value` instead of generated types and typed Meet models, `time` instead of chrono. Done in a scratch copy first with the whole suite ported. | src 1282 to 962 (-320); binary -17.8 %; packages 123 to 108 | kept |
| 2 | `Listing` struct with fn pointers and a two-branch row writer replaced by one `list(meet: bool)` function | part of 1 (`listing.rs` 57 lines to 0, about 25 lines added to `events.rs`) | kept |
| 3 | custom `GetToken` adapter around the authenticator replaced by calling `authenticator.token(&SCOPES)` in `Google::send` | part of 1 (about -24) | kept |
| 4 | `lowercase_initial` helper (11 lines, 2 unit tests) removed: every message starts with our own lowercase context by construction, and the binary-level test checks the real stderr | src -11, tests -33 | kept |
| 5 | typed Meet models (`model.rs`, 68 lines) replaced by JSON pointers on `Value` | part of 1 (-68, about +12 for the pointer lookups) | kept |
| 6 | `calendar get` prints the response through a generic `flatten` (dotted keys, indexed arrays) instead of per-field printing of a typed event | `calendar/get.rs` 96 to 17 + `flatten` 15 | kept (the output format changed and is documented) |
| 7 | `Paths` with public fields instead of the accessors `config_dir()` and `data_dir()` | src 943 to 937 (-6) | kept |
| 8 | test helper `mount_json` implemented by `mount_status` | tests -5 | kept |
| 9 | merge `net.rs` into `google.rs` | src 937 to 934 (-3) | kept |
| 10 | merge `auth/secret.rs` into `auth/setup.rs` | src 934 to 930 (-4) | kept |
| 11 | `with_deadline` takes an `Option<Duration>` so `run` has no `match` | src -4 | kept |
| 12 | delete `typed_path` (quote stripping and `~/` expansion for the prompt answer) | src -7, tests -25 | reverted: `read_line` does not expand `~`, so the most common input (`~/Downloads/client_secret_....json`) would fail with a confusing "cannot read" error |
| 13 | remove the `about` / `help` strings of the clap definition | src -10 | reverted: `--help` is how an agent discovers the commands and the meaning of the id argument |
| 14 | drop the clap features `usage` and `error-context` | binary -41 KB, no line change | reverted: error messages for unknown flags and invalid values lose the offending text (two tests fail) |
| 15 | drop the direct `http` dependency by importing its types through the `hyper` re-export of yup-oauth2 | no line gain | not applied: it would couple HTTP types to the OAuth crate for one Cargo.toml line |
| 16 | drop `serde` as a direct dependency | Cargo.toml -1 | kept (only `serde_json::Value` is used) |
| 17 | pedantic fixes: `match` on bool to `if`, `clone_into`, rebuilt string instead of `push_str(&format!)`, `Box::pin` of the large `execute` future, `meet get` split into `details` plus printing | src about +14 | kept (lint cleanliness) |
| 18 | new behaviour added in this wave: per-call Meet 403/404 tolerance, `conference: none`, the event id validator, `deadline_for`, the closed-pipe rule | src about +30 | kept (required by the findings) |

Tried and not possible: replacing wiremock by httpmock (no access to received requests, see `dependencies.md`); merging `auth/mod.rs` into `token.rs` (it is 9 lines and only declares modules and the scope list).

The net result in `src/` is a decrease of 353 lines even after 17 and 18, so the rule that the line count goes down overall holds.

## Wave 3: new features and what they cost

Before this wave `src/` 929, `tests/` 2042 (non-blank, after `cargo fmt`). After: `src/` 1304 (+375), `tests/` 2928 (+886). The growth is the feature set the owner asked for: `--json` on five commands, `--past` / `--upcoming`, `docs get --tab` (tab list, resolution, redirect-aware export) and `docs metadata`. Per file (`src/`, before to after): `docs.rs` 26 to 171 (+145: tab tree 20, resolution and error messages 45, metadata 45, export 10, id handling), `google.rs` 151 to 204 (+53: two more roots, redirect predicate, `export_tab`, message cap, `get`/`checked` split), `output.rs` 37 to 99 (+62: `compact`, the generic record renderer, `table`; the old `row`, `field` and `flatten` are gone), `calendar.rs` 17 to 69 (+52: the curated event record replaces the generic flatten), `cli.rs` 55 to 82 (+27), `events.rs` 172 to 193 (+21: counts, table output), `lib.rs` 55 to 64, `paths.rs` 44 to 54 (+10: `resolve` takes the environment as a parameter so the passwd-home fallback is testable without touching the process environment), `meet.rs` 199 to 195 (-4: one record replaces the line list).

| # | Experiment | Delta measured | Result |
|---|---|---|---|
| 19 | one generic record renderer (`output::record`) instead of per-command text formatting, ordered records via `serde_json` `preserve_order` | the commands needing text share 45 lines instead of each formatting its own lines (`meet.rs` -4, `calendar get` no longer needs per-field printing) | kept (the `foldhash` side effect is recorded in `dependencies.md`) |
| 20 | `output::row` removed, `table()` writes the cells directly | part of 19 | kept |
| 21 | `now()` as `now_utc().replace_nanosecond(0)` | src -2 | kept |
| 22 | inline `instant()` into `has_ended` and `render_time` | src 0 (the helper moved, no line saved) | reverted |
| 23 | `Unavailable` struct instead of threading `&mut Option<String>` through the helpers | part of the `meet.rs` rewrite (-4 together with the record model) | kept |
| 24 | test helper `Harness::fail_all` instead of six hand-built catch-all mocks | tests -15 | kept |
| 25 | test helper `isolated()` in `tests/binary.rs` | tests -1 | kept |
| 26 | table-driven rewrite of the `tests/meet.rs` list tests | not applied: the tests differ in data and assertions, an estimated -30 lines against a loss of readability | not done |
| 27 | honour `?tab=` of pasted docs URLs (optional item of the brief) | 0 lines | skipped: the value is ignored and `--tab` is the documented way |

## Wave 3b: live-check fixes

Before: `src/` 1304, `tests/` 2928. After: `src/` 1323 (+19), `tests/` 3147 (+219). Per file (`src/`): `output.rs` 99 to 110 (+11: `quoted` and `pairs` replace `leaves`, so every object value is `name=value` pairs), `cli.rs` 82 to 88 (+6: the blank-tab value parser), `google.rs` 204 to 209 (+5: the second accepted Markdown content type), `events.rs` 193 to 190 (-3: one page-size expression replaces the two code paths). The test growth is the independent equivalence oracle in `tests/common` (line-by-line comparison, replacing the weaker substring check), the three `should_panic` tests that prove the oracle notices drift, the allowlist table test, the content-type table test and the page-size tests.

| # | Experiment | Delta measured | Result |
|---|---|---|---|
| 28 | one page-size expression `max(floor).min(250)` for calendar (floor 1) and meet (floor 25) instead of a fixed meet size | src -3 | kept |
| 29 | remove the old `leaves` helper (positional values) in favour of `pairs` | part of the +11 in `output.rs` (the helper was 9 lines, `pairs` and `quoted` are 20) | kept (readability is the owner's priority) |
| 30 | drop the substring-based `every_leaf_of_the_record_appears_in_the_text` test, superseded by the line-by-line oracle | tests -14 | kept |

