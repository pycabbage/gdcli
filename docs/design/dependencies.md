# Dependencies

Audit date: 2026-11-18; the dependencies added for `gdcli update` (below) were audited on 2026-11-19 with the same thresholds. Every direct dependency was checked on four axes against crates.io, GitHub and RustSec; the thresholds and the result are recorded here so the audit can be repeated.

## Thresholds

- Fame: PASS if at least 500k downloads in the last 90 days or 100 reverse dependencies; WARN from 100k downloads or 20 reverse dependencies; else FAIL.
- Stars: PASS if at least 500 stars or the repository belongs to an official organisation (rust-lang, tokio-rs, hyperium, serde-rs, clap-rs, rustls, servo, googleapis, time-rs, chronotope); WARN from 100 stars; else FAIL.
- Maintenance: FAIL if the repository is archived, no non-bot commit author in the last 12 months, or a RustSec "unmaintained" advisory or a maintainer deprecation applies; PASS if organisation-owned or at least 2 active contributors; WARN for a single active contributor in a user-owned repository.
- Update frequency: PASS if the latest release is at most 12 months old (or the last commit at most 6 months old for a mature crate: at least 5 years and 10M downloads); WARN up to 24 months; else FAIL.
- Advisories: any open RustSec advisory for the locked version fails the crate. Overall = worst axis.

## Direct dependencies kept

| Crate | Version | Downloads 90 d | Stars | Owner | Active contributors 12 m | Last release | Verdict |
|---|---|---|---|---|---|---|---|
| anyhow | 1.0.104 | 297M | 6,706 | dtolnay (user) | 3 | 2026-07-18 | PASS (one author writes 95 % of the commits) |
| clap | 4.6.8 | 292M | 16,885 | clap-rs (org) | 43 | 2026-11-16 | PASS |
| http | 1.5.0 | 338M | 1,391 | hyperium (org) | 21 | 2026-07-29 | PASS |
| http-body-util | 0.1.5 | 220M | 173 | hyperium (org) | 5 | 2026-08-12 | PASS |
| hyper-rustls | 0.27.10 | 231M | 362 | rustls (org) | 5 | 2026-09-20 | PASS |
| hyper-util | 0.1.21 | 221M | 182 | hyperium (org) | 31 | 2026-09-24 | PASS |
| serde_json (`preserve_order`) | 1.0.154 | 441M | 5,672 | serde-rs (org) | 9 | 2026-10-11 | PASS |
| time | 0.3.55 | 251M | 1,352 | time-rs (org) | 10 | 2026-08-01 | PASS (jhpratt writes 96 % of the commits) |
| tokio | 1.54.1 | 313M | 33,716 | tokio-rs (org) | 188 | 2026-10-21 | PASS |
| url | 2.5.8 | 268M | 1,590 | servo (org) | 14 | 2026-01-05 | PASS |
| yup-oauth2 | 12.1.2 | 9.0M | 246 | dermesser (user) | 2 | 2026-01-07 | WARN (accepted, below) |
| foldhash (transitive, new in this wave) | 0.2.0 | 350M | 392 | orlp (user) | 1 (not the owner) | 2025-08-23 | WARN (accepted, below) |
| flate2 (`zlib-rs` backend) | 1.1.11 | 240.1M | 1,133 | rust-lang (org) | 26 | 2026-11-06 | PASS |
| ring (direct since `update`; already in the tree through rustls) | 0.17.14 | 235.4M | 4,126 | briansmith (user) | 7 | 2025-03-11 | PASS through the mature-crate clause (last commit 2026-11-07) |
| self-replace | 1.5.1 | 4.07M | 823 | mitsuhiko (user) | 2 | 2026-10-26 | PASS (3 of the 4 commits of the last year are one author's; used by uv, pixi, mise, axoupdater) |
| semver | 1.0.28 | 295.3M | 675 | dtolnay (user) | 2 | 2026-04-04 | PASS (one author writes 90 % of the commits) |
| tar (no default features) | 0.4.46 | 81.8M | 748 | composefs (org; moved from alexcrichton/tar-rs) | 8 | 2026-05-18 | PASS (`>= 0.4.45` is required, see below) |
| tempfile (a normal dependency since `update`, before a dev dependency) | 3.27.0 | 262.0M | 1,474 | Stebalien (user) | 6 | 2026-03-11 | PASS (one author writes 88 % of the commits) |
| wiremock (dev) | 0.6.5 | 36.6M | 805 | LukeMathWalker (user) | 0 | 2025-08-24 | FAIL (accepted, dev only, below) |

The contributor counts of the rows from flate2 to tempfile (added for `gdcli update`) are distinct non-bot commit author names since 2025-11-19 (`gh api "repos/<owner>/<repo>/commits?since=2025-11-19T00:00:00Z&per_page=100" --paginate`, authors whose GitHub type is `Bot` excluded); the earlier rows keep the method of the first audit.

All locked versions equal the latest release on crates.io, none is yanked, and every version range is on the current major or minor line. Features are trimmed: clap, hyper-util, hyper-rustls, tokio, time, yup-oauth2, flate2 (`zlib-rs` backend) and tar use `default-features = false`.

## Packages added by `gdcli update`

The normal tree grows from 109 to 121 packages (+12): the five new direct crates `self-replace`, `tempfile` (dev to normal), `tar`, `flate2` and `semver`, plus the seven below. `Cargo.lock` grows from 154 to 160 packages (the other six were already locked through dev dependencies). `ring` was already in the tree. The `zlib-rs` backend of `flate2` keeps `miniz_oxide`, `adler2`, `crc32fast` and `simd-adler32` out of the tree. Numbers measured on 2026-11-19 (downloads are the crates.io 90 day figure; contributors are distinct non-bot commit author names since 2025-11-19, counted as described under the direct dependencies).

| Package | Version | Pulled by | Downloads 90 d | Stars | Owner | Active contributors 12 m (distinct non-bot commit author names since 2025-11-19) | Last release | Verdict |
|---|---|---|---|---|---|---|---|---|
| zlib-rs | 0.6.8 | flate2 | 143.8M | 718 | trifectatechfoundation (org) | 24 | 2026-09-15 | PASS |
| fastrand | 2.5.0 | tempfile | 306.0M | 605 | smol-rs (org) | 6 | 2026-07-19 | PASS |
| rustix | 1.1.6 | tempfile | 377.1M | 2,165 | bytecodealliance (org) | 29 | 2026-10-27 | PASS |
| bitflags | 2.13.2 | rustix | 607.5M | 1,175 | bitflags (org) | 11 | 2026-09-10 | PASS |
| getrandom | 0.4.3 | tempfile | 838.1M | 595 | rust-random (org) | 10 | 2026-06-17 | PASS (second version next to ring's 0.2.17, accepted duplicate) |
| filetime | 0.2.30 | tar | 118.4M | 132 | alexcrichton (user) | 4 | 2026-11-16 | WARN (accepted, below) |
| linux-raw-sys | 0.12.1 | rustix | 356.0M | 72 | sunfishcode (user) | 5 | 2025-12-23 | FAIL by star count (accepted, below) |

## Decisions

| Crate | Decision | Reason |
|---|---|---|
| chrono | replaced by `time` | RUSTSEC-2026-0543 (unmaintained, soft-deprecated by its maintainer). `time` passes every axis and was already in the tree through yup-oauth2 (with the parsing and formatting features), so it adds no package. |
| google-apis-common, google-calendar3, google-drive3 | replaced by `src/google.rs` | RUSTSEC-2025-0066: google-apis-rs is unmaintained (the maintainer announced the archival of the project); Calendar and Drive clients have a single maintainer and the Calendar crate has 5 reverse dependencies. No maintained library exists for Calendar v3 or Drive v3 (googleapis/google-cloud-rust has no Calendar or Drive crate, and its Meet crate `google-apps-meet-v2` has no `fields` support and needs its own HTTP and auth stack: +60 packages, +3.07 MB binary, +2.6 MB peak RSS when measured). One generic GET layer on the already trusted hyper stack (about 150 lines including the HTTPS plumbing) replaces all of them. Measured effect: binary 4,334,864 to 3,565,016 bytes (-17.8 %), packages 123 to 108, peak RSS of a request about 0.4 MB lower, `src/` 1282 to 929 lines. |
| xdg | replaced by 6 lines in `src/paths.rs` | no commit or release in 18.5 months and no active contributor; the XDG rules we need (absolute `$XDG_*_HOME`, else `$HOME/.config` and `$HOME/.local/share`, relative values ignored) are a few lines of std. Tested at binary level (relative value ignored, missing HOME). |
| serde | no longer a direct dependency | the code only uses `serde_json::Value`; serde stays in the tree through serde_json and yup-oauth2. |
| wiremock | kept (dev only) | no commit or release in 14.8 months, one owner, but 36.6M downloads, 805 stars, no advisory, never shipped. `httpmock` 0.9 (all axes PASS) was evaluated: it supports dynamic responders, delays and custom matchers, but offers no access to the received requests (only call counts), while the suite asserts the exact query of every request (`fields`, `eventTypes`, filters, `pageSize`); porting would turn those assertions into per-mock matchers that cannot detect unexpected extra requests or parameters. Not ported. |
| directories, dirs, etcetera | rejected | `directories` is archived; `dirs` adds two packages and changes the HOME fallback behaviour; `etcetera` is WARN and not better than six lines of std. |
| google-cloud-auth, oauth2, gcp_auth | rejected for login | none provides the installed-app loopback flow with token persistence (`google-cloud-auth` has no interactive flow, `oauth2` is primitives only and stale, `gcp_auth` is service-account/ADC only). |
| self_update, ubi, axoupdater, binstalk, sigstore-verify | rejected for `gdcli update` | `self_update`: +2,451,448 bytes (+67 %), +39 packages, a second HTTP client (`ureq`), mandatory `regex`, ignores draft, prerelease and immutable releases; `ubi`: +3.45 MB, +86 packages, a `chrono` unmaintained advisory (RUSTSEC-2026-0543) in its tree, in-place copy fails with ETXTBSY; `axoupdater`: needs `dist` install receipts and an installer asset; `binstalk`: +94 packages; `sigstore-verify`: +34 packages including `aws-lc-sys` (C). The update is our hyper/rustls stack plus the crates above (details in `update.md`). |
| sha2 | rejected, `ring::digest` | `ring` is already in the tree; `sha2` would add `digest`, `block-buffer`, `crypto-common`, `generic-array`, `cpufeatures` and a second SHA-256 |
| a manual version compare, a hand-written replace | rejected, `semver` and `self-replace` | the owner prefers libraries to scratch code; `self-replace` is 48 lines on Unix and needs `tempfile`, which gdcli uses anyway |
| `miniz_oxide` backend of flate2 | rejected, `zlib-rs` | 3 packages fewer, faster inflate, 12.5 KB more binary |

## Accepted risks and exit plans

- serde_json `preserve_order` (key order of the JSON records is the output contract) enables indexmap's hasher feature, which adds one package to the tree: `foldhash` 0.2.0 (the default hasher of hashbrown, 350M downloads in 90 days, 416 reverse dependencies, 392 stars, single user owner, last release 2025-08-23). Verdict WARN (stars and release age), no advisory. Exit plan: build the records as an ordered `Vec<(&str, Value)>` and drop the feature (an estimated 40 more lines).
- yup-oauth2 (WARN: 246 stars, user-owned repository, 7 commits in 12 months, none in the last 10): it is the only maintained crate with the installed-app flow, loopback redirect, token persistence and refresh, and it has two crates.io owners and 762 reverse dependencies. Trigger to leave: a RustSec advisory, breakage against Google's OAuth endpoints with no fix within 60 days, or no release for 24 months (January 2028). Plan: own installed flow on top of `hyper-util` and `serde_json` (loopback listener, code exchange, refresh, 0600 token file), an estimated 100 to 150 lines of security-sensitive code.
- wiremock (dev only, FAIL on maintenance): exposure is the CI build and test only. Trigger: it stops compiling on a new toolchain or an advisory appears. Plan: port the harness to `httpmock` (or `mockito`) and reduce the received-request assertions to matchers.
- The REST layer is ours: we own the Calendar v3, Drive v3 and Meet v2 request and response handling (about 150 lines in `src/google.rs` plus the field masks). Trigger to revisit: Google or a trusted organisation publishes maintained Rust clients for these APIs that support partial responses and lean dependencies.
- `linux-raw-sys` (new, FAIL by star count: 72 stars, user-owned): accepted. It is an unavoidable transitive of `rustix`, which `tempfile` needs, which `self-replace` needs; it has the same author as `rustix` (sunfishcode, bytecodealliance), 356M downloads in 90 days, a release 11 months ago and no advisory, and it was already in `Cargo.lock` through the dev dependencies. Owner acknowledgement requested. Trigger to leave: a RustSec advisory or no release for 24 months (December 2027). Plan: drop `self-replace` and `tempfile` for about 20 lines of `std` (create the staging file with `OpenOptions::create_new` next to the executable, copy, set permissions, rename).
- `filetime` (new, WARN: 132 stars, user-owned, one dominant author): accepted, it is required by `tar` and tiny (118M downloads). It follows the `tar` exit plan.
- `self-replace` (3 of 4 commits by one author) and `tar` (the repository moved to the composefs organisation): exit plans are the `std` replacement above and, for `tar`, `astral-tokio-tar` or a header parser on top of `flate2`; trigger for both: a RustSec advisory or no release for 24 months. `tar` must stay at `>= 0.4.45`: RUSTSEC-2026-0067 concerns `unpack_in`, which is never called, and RUSTSEC-2026-0068 concerns PAX size-header parsing, which `Archive::entries()` uses.
- Duplicate package version: `getrandom` 0.2.17 (ring) and 0.4.3 (tempfile) are both shipped (the first duplicate in the shipped tree); it goes away when ring and tempfile use the same `getrandom`.

## Transitive findings

- `cargo audit` (RustSec database of 1533 advisories fetched on 2026-11-19, 160 crates in `Cargo.lock`): 0 vulnerabilities, 0 warnings; it also passes with `-D warnings`, so it can be added to CI without an ignore list.
- `cargo tree -d -e normal`: one duplicate in the shipped tree, `getrandom` 0.2.17 and 0.4.3 (accepted above).
- `cargo machete`: no unused dependency.
- 121 unique packages in the normal tree, including gdcli (109 before `gdcli update`, 108 before `foldhash`). Notes from the audit that remain true: `cfg-if` (via ring) is archived upstream without an advisory; `lazy_static` (dev only, via wiremock) is archived; about 14 tiny feature-complete crates have had no release for more than 18 months (`fnv`, `httpdate`, `seahash`, `untrusted`, `httparse`, `subtle`, `tower-service`, `ring` 0.17.14 whose upstream is active, ...), none has an advisory; `base64` 0.22 is locked while 0.23 exists, chosen by yup-oauth2.

## How to re-audit

```
cargo audit --db <advisory-db> -D warnings
cargo tree -e normal --prefix none --no-dedupe | sed 's/ (.*//' | sort -u | wc -l
cargo tree -d -e normal
cargo machete
curl -s -A gdcli https://crates.io/api/v1/crates/<crate>
gh api repos/<owner>/<repo> --jq '.stargazers_count,.archived,.pushed_at'
gh api 'repos/<owner>/<repo>/commits?since=<date 12 months ago>&per_page=100' --paginate
```

`cargo audit` and `cargo machete` are installed privately (`CARGO_INSTALL_ROOT=<dir> cargo install cargo-audit cargo-machete --locked`), not in the repository.
