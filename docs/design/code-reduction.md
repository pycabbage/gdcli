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

## Wave 3c: reviewer nits

Before: `src/` 1323, `tests/` 3147. After: `src/` 1324 (+1), `tests/` 3167 (+20).

| # | Experiment | Delta measured | Result |
|---|---|---|---|
| 31 | case-insensitive content-type compare via `map(str::to_ascii_lowercase)` | src +1 | kept |
| 32 | delete the nested-object arm of `pairs` (records have at most one level of nesting) and let a nested object fall into the quoted-JSON arm | src 0 (the arm changed from recursion to one quoted-JSON `push`; a pure deletion would silently drop an unexpected object, which the owner's equivalence rule forbids) | kept, now pinned by a test |

## Wave 4: `gdcli update`

Before: `src/` 1324, `tests/` 3173 (non-blank, after `cargo fmt`). After: `src/` 1551 (+227: `update.rs` 223, `cli.rs` 2, `lib.rs` 2; `wc -l` of `update.rs` is 235), `tests/` 3908 (+735: `update.rs` 22 tests with the fixture, 4 unit tests, three extended assertions).

| # | Experiment | Delta | Result |
|---|---|---|---|
| 33 | `Asset` struct replaced by a `(url, sha256)` tuple, `Fetcher::new` removed in favour of public fields, the install directory derived from `current_exe()` in one expression | `update.rs` 247 to 238 lines (`wc -l`, measured together with the three changes) | kept |
| 34 | one `github` predicate using `url.authority() == host` instead of separate userinfo, port and scheme checks | not measured separately (the three checks would be about 3 lines longer) | kept |
| 35 | shared `https_host` helper with `google::googleusercontent` | estimated -1 line and an edit of a tested Google path | not done |
| 36 | the reset time in the rate-limit message (`time` is already a dependency) | not measured separately (about 7 lines) | kept: it tells an agent when to retry |
| 37 | `self-replace` and `semver` instead of hand-written replace and version compare | not written, estimated 12 and 8 lines | kept |
| 38 | `--check`, `--json` | not written | not done |
| 39 | review fixes: context on the download write, one neutral extraction message, `Fetcher::archive` public for a test, its `sha256` parameter renamed `want` so the signature and the checksum `ensure!` each fit on one line | `update.rs` 238 to 235 (`wc -l`, measured) | kept |
| 40 | the staged-file creation in `extract` gets the same `cannot extract the archive` context (`commands.md` promises it for a full disk after the download) | `update.rs` stays at 235 lines (`wc -l`, measured) | kept |

## Wave 5: `GITHUB_TOKEN`

Before: `src/` 1551, `tests/` 3908 (non-blank, after `cargo fmt`). After: `src/` 1579 (+28, all in `update.rs`: 223 to 251 non-blank, `wc -l` 235 to 265), `tests/` 4094 (+186: 5 new tests in `update.rs`, 2 unit tests, the rate-limit test extended).

| # | Experiment | Delta | Result |
|---|---|---|---|
| 41 | the token as a parameter of `send` and `ok` (no field on `Fetcher`, origin comparison instead of a flag per hop) | not measured separately (inside the +28) | kept |
| 42 | `github_token` and `bearer` as `pub` helpers, so that the empty, non-unicode, sensitive and invalid cases are unit tests and not tests of the process environment | not measured separately (inside the +28) | kept |
| 43 | one `reset` string shared by the two rate-limit messages instead of separate `match` arms | not measured separately (inside the +28) | kept |

## Wave 6: review fixes of `GITHUB_TOKEN` and the module split

Before: `src/` 1579, `tests/` 4094 (non-blank, after `cargo fmt`). After: `src/` 1591 (+12), `tests/` 4160 (+66: two new tests in `update.rs`, one more row in the status test, one changed unit test). `src/update.rs` (265 lines, the largest module of the repository) became `src/update/mod.rs` (138 lines, 129 non-blank) and `src/update/fetch.rs` (142 lines, 134 non-blank); the sum is 280 lines because the second file repeats its imports and the parent has `mod` and `pub use` lines.

| # | Experiment | Delta | Result |
|---|---|---|---|
| 44 | split by responsibility: everything that touches the network or the token in `fetch.rs`, the flow and the pure selection in `mod.rs` | 265 lines in one file to 138 + 142 | kept |
| 45 | `send` returns whether the credential was still attached on the final answer; the 401 and rate-limit wording follows that bit and the credential is dropped for the rest of the chain after the first foreign hop | not measured separately (inside the +12) | kept |
| 46 | `github_token` reads a non-unicode value lossily instead of dropping it | 0 lines | kept: a broken value is rejected by GitHub instead of silently falling back to anonymous |

## Wave 7: accounts and the `auth` group (v0.4.0, work package 1)

Before: `src/` 1591, `tests/` 4160 (non-blank, after `cargo fmt`). After: `src/` 1899 (+308), `tests/` 5237 (+1,077: `tests/accounts.rs` 31 tests, 733 lines; 9 more tests in `binary`, `cli` and `units`; the hermetic `env_clear()` helpers of `binary.rs` and `auth.rs`). Per file (`src/`, before to after): `auth/accounts.rs` 0 to 203, `cli.rs` 90 to 126 (+36: `AccountArg`, `AuthCommand`, four flattened options), `auth/mod.rs` 9 to 40 (+31: `auth list`, `auth set-default`, the login rendering), `paths.rs` 54 to 66, `lib.rs` 66 to 76 (+10: lazy path resolution), `token.rs` 81 to 90, `google.rs` 210 to 217. The first estimate was +250 for `src/`; it was made before `rustfmt` expanded the closures and error arms.

| # | Experiment | Delta | Result |
|---|---|---|---|
| 47 | one validated `Email` newtype (a shape check of about 6 lines) instead of a regex or an email crate | no dependency added, 121 packages | kept |
| 48 | one `absent()` helper builds the `no account`, `unknown account`, `default account has no token` and `no default account` errors for `token_file`, `set_default` and `default_account` | not measured separately | kept |
| 49 | `read_dir` NotFound handled by one `match` arm that falls through to `with_context` | src 214 to 212 (-2, measured) | kept |
| 50 | the rendering of `auth login` (`render`) moved from `accounts.rs` to `auth/mod.rs` next to `auth list` and `auth set-default` | src 336 to 333 over the three files (-3, measured) | kept |
| 51 | path resolution through a `resolve` function so that `update` never needs `HOME` | not measured separately (inside the +10 of `lib.rs`) | kept |
| 52 | `auth login` reads the client secret before it creates `accounts/` (review fix) | src 0 (two lines swapped) | kept: a failing login creates nothing |

## Wave 8: scenario features (v0.4.0, work package 2)

Before: `src/` 1899, `tests/` 5237 (non-blank, after `cargo fmt`). After: `src/` 2126 (+227), `tests/` 7016 (+1,779: `tests/range.rs` 558 and `tests/meet_read.rs` 558 non-blank lines, 41 new tests in all; `rustfmt` puts every long argument list and every array of command lines on its own lines, which is most of the growth). Per file (`src/`, before to after): `cli.rs` 126 to 189 (+63: `moment`, `query_text`, four options with their `rustfmt`-expanded attributes, `check`, `ReadArgs`, two commands), `events.rs` 190 to 239 (+49: `Window`, `first`, `attachments`, range mode, the `when` decision per row), `docs.rs` 171 to 177 (+6: `response` and `copy` split out of `get`, two imports), `calendar.rs` 69 to 64 (-5: `each` moved to `events.rs`), `lib.rs` 76 to 86 (+10: two `check()` calls, two read arms), `meet.rs` 195 to `meet/` 299 (+104: `mod.rs` 54, `records.rs` 179, `read.rs` 66). The plan estimated +200 for `src/`; the difference is the `rustfmt` layout of attribute lists, struct literals and the 5-tuples of `ARTIFACTS`.

| # | Experiment | Delta | Result |
|---|---|---|---|
| 53 | `upcoming()` replaced by `first()`, one function for the upcoming side of the default mode and for range mode (the caller adds `!has_ended` as part of the predicate) | not measured separately (a second collector with the same body was never written) | kept |
| 54 | rows take `when` from the event (`has_ended`) instead of from the list they come from, so one loop renders default and range mode | not measured separately (the nested loop over the two lists is gone) | kept |
| 55 | one `document` mapper for `transcripts` and `smartNotes` (both are `{state, startTime, endTime, docsDestination/document}`) instead of a separate `smart_note` function | not measured separately (the mapper exists once) | kept |
| 56 | `each` moved from `calendar.rs` to `events.rs` and `attachments` written once on top of it for `calendar get` and `meet get` | `calendar.rs` -5 (measured), the body reappears in `events.rs` with `attachments` on top | kept |
| 57 | `Window { from, to, query }` instead of three more positional parameters of `pages` (the signature stays under clippy's argument limit) | not measured separately (the struct plus three literals that `rustfmt` lays out on five lines each) | kept |
| 58 | `ARTIFACTS` entries are 5-tuples (collection, record key, `fields`, page size, mapper) because the Meet collection `smartNotes` is the JSON key `smart_notes` | not measured separately (the tuples are longer, no helper function) | kept |
| 59 | `docs::get` split into `response` and `copy` so that `meet notes` and `meet transcript` reuse the export (one function body moved, nothing duplicated) | `docs.rs` +6 | kept |
| 60 | `meet.rs` (about 295 lines with the new code) split into `meet/mod.rs`, `records.rs` and `read.rs` by responsibility (commands, shared Meet resources, the two read commands) | largest module of the repository stays `events.rs` (239 non-blank lines); `records.rs` 179 | kept |
| 61 | `--to` and `--limit` get `conflicts_with_all = ["past", "upcoming"]` besides `requires = "from"`: clap does not report the missing `--from` when a conflicting option is present, so `--past 3 --to X` would have parsed and ignored `--to` (found by `to_and_limit_require_from`) | +2 attribute lines | kept |
| 62 | a `Harness` helper `mount_doc`, `mount_collection` and `mount_records` in `tests/common` instead of per-file copies (the `mount_records` of `tests/meet.rs` moved) | not measured separately (`tests/common` grew by 52 non-blank lines in all, with the new builders) | kept |
| 63 | the review nits of the accounts test (a mode-restoring drop guard, two closures instead of the half-vacuous `denied`, distinct skip messages) | tests +14, no test added | kept |

## Wave 9: review fixes of the scenario features (v0.4.0, work package 2)

Before: `src/` 2126, `tests/` 7016 (non-blank, after `cargo fmt`). After: `src/` 2133 (+7), `tests/` 7032 (+16); no test was added (236 tests). Per file: `cli.rs` 189 to 191 (+2), `lib.rs` 86 to 84 (-2), `meet/read.rs` 66 to 73 (+7), `tests/range.rs` 558 to 550 (-8), `tests/meet_read.rs` 558 to 582 (+24).

| # | Experiment | Delta | Result |
|---|---|---|---|
| 64 | `ListArgs::check` compares `unix_timestamp()` values: the request carries whole seconds, so `00:00:00.1Z` to `00:00:00.9Z` is an empty range (`timeMin == timeMax`, a Calendar 400) and is now rejected by the same message | `cli.rs` +2 (the expression no longer fits one line); `tests/range.rs` +18 for three more rows of `check_rejects_from_not_before_to_and_equal_bounds` | kept |
| 65 | `meet::notes` and `meet::transcript` take the parsed `ReadArgs`; `read`, `NOTES` and `TRANSCRIPT` became private; the `lib.rs` arms lost the shadowing `(google, tab)` tuple, so the choice of the constant and the `--tab` pass-through are now covered by tests | `read.rs` +7, `lib.rs` -2; `tests/meet_read.rs` +24 (`read_args` parses a real command line; the `--tab` case of the single-document test also runs `meet transcript`) | kept |
| 66 | `day_event(id, title, day, hour)` in `tests/range.rs` replaces the `titled(..)` literals of the two `query_*` tests with the same events | `tests/range.rs` -26 (576 to 550 non-blank), the same assertions | kept |
| 67 | the two non-ASCII literals added by the scenario tests became `\u{..}` escapes | 0 lines | kept |

## Wave 10: corrections from the real-credential run (v0.4.0, work package 2)

Before: `src/` 2133, `tests/` 7032 (non-blank, after `cargo fmt`). After: `src/` 2131 (-2), `tests/` 7032 (0); 236 tests, binary sizes unchanged (3,913,408 bytes x86_64, 3,353,528 bytes aarch64 zigbuild, the same as before the change).

| # | Experiment | Delta | Result |
|---|---|---|---|
| 68 | the `no conference record` message says what Meet does (a record exists from the start of a meeting until 30 days after it) instead of "records expire 30 days after the meeting", which misled for an upcoming meeting; one string in `meet/read.rs`, one constant in `tests/meet_read.rs`, the skill drafts and `commands.md` follow | 0 lines | kept |
| 69 | the `docs get` help text no longer says that the tabs are "concatenated with nothing between them": the whole export is not the byte concatenation of the tab exports (29,559 B against 9,451 B plus 20,557 B in the live check), and `rustfmt` then puts the attribute on one line | `cli.rs` -2 | kept |
| 70 | documentation only: Gemini notes documents have one or two tabs, titles are localized, the real-credential results are recorded in `testing.md` | 0 lines of code | kept |


## Wave 11: the agent skill (v0.4.0, work package 3)

Before: `src/` 2131, `tests/` 7032 (non-blank, after `cargo fmt`). After the review fixes: `src/` 2178 (+47: `src/skills.rs` 24, `cli.rs` 189 to 202 with the `Skills` variant and the `Topic` enum, `lib.rs` 84 to 94 with the writer parameter of `run_with` and `execute`), `tests/` 7614 (+582: `tests/skills.rs` 480, 16 tests; `tests/common/mod.rs` 321 to 453 with the shared fixtures and `mount_full_meeting`; `tests/docs.rs` 486 to 440 and `tests/calendar_get.rs` 211 to 177 after the fixtures moved out; `tests/cli.rs` +43 for one test and three extra rows; `tests/units.rs` +7). Before the review fixes the totals were `src/` 2170 and `tests/` 7598. The skill itself is five files of 225 lines and 19,630 bytes. 253 tests.

| # | Experiment | Delta | Result |
|---|---|---|---|
| 71 | `skills::print` writes a slice of parts in a loop with a `\n` before every part but the first, so `full` needs no joined string and every topic shares one code path | `src/skills.rs` 24 lines in all, no allocation | kept |
| 72 | the five files are embedded with `include_str!` straight from `skills/gdcli/` (no build script, no generated or committed copies, cargo tracks the files) | 0 lines | kept |
| 73 | `Topic` is a derived `ValueEnum` (no hand-written parser, clap prints the possible values and rejects other cases) | `cli.rs` +13 with the variant | kept |
| 74 | the contract tests take their oracle from `Cli::command()` (leaves, options, positionals, `--json` placement) instead of lists in the test, so a new command or option fails the suite until its reference is written | tests only, no list to maintain | kept |
| 75 | the full-event, tab-tree and file fixtures moved into `tests/common` instead of being copied for the key tests | `tests/docs.rs` -46, `tests/calendar_get.rs` -34, `tests/common/mod.rs` +132 with `mount_full_meeting` | kept |
| 76 | error messages in the files are checked by cutting them at their placeholders and searching `src/` for the pieces, instead of keeping a table of messages in the test | one 40-line test, no third copy of any message | kept |
| 77 | `run_with(cli, resolve, out)` takes the output writer and `execute` no longer locks stdout itself (review fix): `run` passes `&mut std::io::stdout().lock()` and the tests pass a `Vec`, so `skills_does_not_resolve_the_paths` compares the captured bytes with `references/docs.md` and counts the resolver calls instead of leaving a reference in the output of `cargo test`; the earlier choice to keep `run_with(cli, resolve)` and accept the noise is withdrawn | `lib.rs` +8 (the signatures), no `&mut out` left in the arms, `tests/units.rs` +7 | kept |
| 78 | the references carry the columns, the key lists with their meaning and the user-visible rules that left `commands.md` (one place for usage), written without literal tab characters; the drafts of 12,396 bytes grew to 19,630 bytes | binary +24,720 bytes for the text (`performance.md`); `commands.md` 255 to 162 lines | kept |
