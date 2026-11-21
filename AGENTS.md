# gdcli

Agent-oriented CLI for Google Workspace: Meet, Calendar and Docs. Rust. This file exists because the owner's INSTRUCTIONS.md is not committed; it transcribes every rule from it.

## Commands

```
gdcli auth setup [--no-browser] auth setup wizard: have the user place the key, then start the OAuth login flow
gdcli auth login [--no-browser] OAuth login flow only (setup without the wizard part): log in one Google account, the first one becomes the default
gdcli auth list          list the logged-in accounts and mark the default
gdcli auth set-default <email> make an account the default
gdcli meet list          latest 10 Google Meet meetings (default past 5, future 5; --past N --upcoming N, alias --future)
gdcli meet get <id>      show Meet meeting info, attachments and Gemini smart notes
gdcli meet notes <id>    print a meeting's Gemini notes as Markdown; --tab <id|name> selects one tab
gdcli meet transcript <id> print a meeting's transcript as Markdown; --tab <id|name> selects one tab
gdcli calendar list      latest 10 calendar events (default past 5, future 5; --past N --upcoming N, alias --future)
gdcli calendar get <id>  get and show an event with attachments
gdcli docs get <id>      get a Google Doc as Markdown; --tab <id|name> selects one tab
gdcli docs metadata <id> metadata of a Google Doc
gdcli skills [topic]     print the agent skill: SKILL.md, or a reference (full, meet, calendar, docs, auth); embedded from skills/gdcli
gdcli update             update gdcli from the latest GitHub release
```

- `--json` (same information as JSON) exists on meet/calendar list and get and on `docs metadata`, never on `docs get`, `meet notes` or `meet transcript` (Markdown exports).
- `--account <email>` (`-a`) exists on every command that talks to Google (all `meet`, `calendar` and `docs` subcommands, eight leaves) and nowhere else (not on `auth *`, `update`); without it the default account is used, and the first login becomes the default.
- `skills/gdcli/` (SKILL.md and references/*.md, installable with `npx skills`) is the single source of truth for how every command is used; `gdcli skills` embeds it at build time with `include_str!`; a change to a command, option, record key or error message updates it in the same commit (`tests/skills.rs` enforces this).
- `meet list` and `calendar list` also take `--from TIME [--to TIME] [--query TEXT] [--limit N]` (range mode, oldest first; `--to` and `--limit` need `--from`; `--query` also works without `--from`); details: `skills/gdcli/references/{meet,calendar}.md` (usage), `docs/design/commands.md` (design).
- `auth setup` and `auth login` open the login URL through the `open` crate (https only) unless `--no-browser`; in an AI agent shell (11 variables, sources in `docs/design/auth.md`) they open nothing and print one hint unless `GDCLI_OPEN_BROWSER=1` (only the value `1`); agents set it only after the user has agreed to a browser opening on their machine, never on their own.
- `docs get`: use an official Markdown-output API if one exists, otherwise write own Markdown conversion. Option `--json` (raw JSON output) exists only if no official Markdown API exists and raw JSON is available; otherwise drop the option.
- Current decision: Drive `files.export` with `text/markdown` is the official Markdown API, so `docs get` has NO `--json`.

## Rules

- Agents may run `git add`, `commit`, `push` and `tag`; never `git config`, never force-push, never rewrite or delete a tag whose release was published.
- Pursue performance above all; optimize memory usage.
- User-touched config (e.g. client_secret.json) lives in `$XDG_CONFIG_HOME/gdcli/`; program-touched state/cache (e.g. token) lives in `$XDG_DATA_HOME/gdcli/`.
- No code comments at all, in any file type (including YAML/TOML). Needed information goes in English docs under `<repo_root>/docs/{design,...}`.
- README.md is for users only, in English: exactly one line describing the project, one 3-line code block (download from releases, extract into `~/.local/bin`, run `gdcli auth setup`), one sentence ("Skills are available via `gdcli skills`, or can be added to your system with:") and one 1-line code block (`npx skills add pycabbage/gdcli`). No contribution info or any other text.
- Comment/README limits are never a reason to drop information from the repo. Keep the repo minimal in bytes yet complete in information (all information in docs/AGENTS.md).
- Split source into appropriately sized modules/hierarchy.
- Write as little source as possible. Search exhaustively (dozens of times) for reliable libraries/frameworks; hand-write code only if none exists.
- After implementing, an independent reviewer (separate from tests and from the implementer) must review it.
- CI workflow: on commit push, build and test.
- Release workflow: on tag push, GoReleaser release for linux amd64 and linux arm64.
- Every workflow must use the latest major version of each action (e.g. `actions/checkout@v7`) and be locked with `gh actions-lock`.
- Never hand-edit tool-generated files (`.github/workflows/actions.lock`, `Cargo.lock`) without a strong reason; the machine-generated headers of `actions.lock` and `Cargo.lock` are the only comment-like text allowed.
- Every GitHub Action used in `.github/workflows` (first-party or third-party) must have at least 1000 stars, checked with `gh api repos/<owner>/<repo> --jq .stargazers_count`; transitive actions inside an accepted wrapper are recorded in `docs/design/release.md`.
- Commit only after the reviewer passes.
- Git: author/credentials are preconfigured. NEVER run any `git config` command; NEVER read or write `.git/config`.
- Tags and releases are managed by agents (tag = `v` + Cargo.toml version; procedure in `docs/design/release.md`). End-to-end tests with real credentials run only with the owner's installed credentials. Real-credential checks run only on copies of the credential files in a scratch directory outside the repository, and a gdcli binary is only run under `env -i` with a scratch `HOME` and `XDG_*` (rules and reporting limits: `docs/design/testing.md`).
- Violating these instructions may get the deliverables deleted.

## Docs index

- `docs/setup.md`
- `docs/design/`
- `skills/gdcli/`
