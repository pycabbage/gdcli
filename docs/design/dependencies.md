# Dependencies

Audit date: 2026-11-18. Every direct dependency was checked on four axes against crates.io, GitHub and RustSec; the thresholds and the result are recorded here so the audit can be repeated.

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
| tempfile (dev) | 3.27.0 | 260M | 1,474 | Stebalien (user) | 6 | 2026-03-11 | PASS (one author writes 88 % of the commits) |
| wiremock (dev) | 0.6.5 | 36.6M | 805 | LukeMathWalker (user) | 0 | 2025-08-24 | FAIL (accepted, dev only, below) |

All locked versions equal the latest release on crates.io, none is yanked, and every version range is on the current major or minor line. Features are trimmed: clap, hyper-util, hyper-rustls, tokio, time and yup-oauth2 use `default-features = false`.

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

## Accepted risks and exit plans

- serde_json `preserve_order` (key order of the JSON records is the output contract) enables indexmap's hasher feature, which adds one package to the tree: `foldhash` 0.2.0 (the default hasher of hashbrown, 350M downloads in 90 days, 416 reverse dependencies, 392 stars, single user owner, last release 2025-08-23). Verdict WARN (stars and release age), no advisory. Exit plan: build the records as an ordered `Vec<(&str, Value)>` and drop the feature (an estimated 40 more lines).

- yup-oauth2 (WARN: 246 stars, user-owned repository, 7 commits in 12 months, none in the last 10): it is the only maintained crate with the installed-app flow, loopback redirect, token persistence and refresh, and it has two crates.io owners and 762 reverse dependencies. Trigger to leave: a RustSec advisory, breakage against Google's OAuth endpoints with no fix within 60 days, or no release for 24 months (January 2028). Plan: own installed flow on top of `hyper-util` and `serde_json` (loopback listener, code exchange, refresh, 0600 token file), an estimated 100 to 150 lines of security-sensitive code.
- wiremock (dev only, FAIL on maintenance): exposure is the CI build and test only. Trigger: it stops compiling on a new toolchain or an advisory appears. Plan: port the harness to `httpmock` (or `mockito`) and reduce the received-request assertions to matchers.
- The REST layer is ours: we own the Calendar v3, Drive v3 and Meet v2 request and response handling (about 150 lines in `src/google.rs` plus the field masks). Trigger to revisit: Google or a trusted organisation publishes maintained Rust clients for these APIs that support partial responses and lean dependencies.

## Transitive findings

- `cargo audit` (RustSec database of 1527 advisories, 153 crates in `Cargo.lock`): 0 vulnerabilities, 0 warnings; it also passes with `-D warnings`, so it can be added to CI without an ignore list.
- `cargo tree -d -e normal`: no duplicate versions in the shipped tree.
- `cargo machete`: no unused dependency.
- 109 unique packages in the normal tree, including gdcli (one more than before: foldhash, see above). Notes from the audit that remain true: `cfg-if` (via ring) is archived upstream without an advisory; `lazy_static` (dev only, via wiremock) is archived; about 14 tiny feature-complete crates have had no release for more than 18 months (`fnv`, `httpdate`, `seahash`, `untrusted`, `httparse`, `subtle`, `tower-service`, `ring` 0.17.14 whose upstream is active, ...), none has an advisory; `base64` 0.22 is locked while 0.23 exists, chosen by yup-oauth2.

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
