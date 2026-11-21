---
name: gdcli
description: Read Google Calendar events, Google Meet meetings with their transcripts and Gemini notes, and Google Docs as Markdown through the gdcli command line tool. Use when asked about meeting schedules, what was said or decided in a meeting, meeting notes or summaries, transcripts, or the content of a Google Doc, and when switching between Google accounts.
---

# gdcli

Agent-oriented CLI for Google Workspace (Meet, Calendar, Docs), read-only. Output is plain text: `key: value` lines, `name=value` pairs, tab-separated lists with a header line, or Markdown for documents. An installed copy of this text can be older than the binary: `gdcli skills` prints the text of the binary and is authoritative.

## Before the first command

- `gdcli auth list` prints the logged-in accounts (columns `account`, `default`). With no account a human must run `gdcli auth setup` (first time) or `gdcli auth login` (more accounts): both need a browser, so do not run them yourself. If a human asks you to run one, pass `--no-browser` (an agent that gdcli does not detect, and Claude Code before v2.1.172, would otherwise open a browser on the user's machine), give the printed URL to the human, and set `GDCLI_OPEN_BROWSER=1` only after the user has agreed to a browser opening on their machine, never on your own.
- Every Google command takes `-a, --account <email>`; without it the default account is used.
- Errors go to stderr as `error: <message>` with exit status 1 and an empty stdout (a document that breaks mid-stream may leave partial output); usage errors exit with 2.
- Never read or print `client_secret.json` or the files under `$XDG_DATA_HOME/gdcli`; they hold credentials.

## Scenario: what happened in a meeting

1. Find the meeting. Latest matches: `gdcli meet list --query standup --past 10 --upcoming 0` (looks back at most 730 days; use `--from` for older matches). A period: `gdcli meet list --from 2026-11-01 --to 2026-11-20 --query standup` lists the first matches, oldest first, at most `--limit` (default 50); exactly that many rows may mean more exist, so narrow the period. A bare date is a UTC day; for a Japan day write `--from 2026-11-20T00:00:00+09:00`. The first column `id` is the Calendar event id: the argument of `meet get`, `meet notes`, `meet transcript` and `calendar get`.
2. `gdcli meet get <id>` shows the space, the conferences and their artifacts: `transcript:` and `smart_note:` lines carry a `document_id`. `gdcli calendar get <id>` and `meet get` list `attachment:` lines with a `file_id`.
3. Read the Gemini notes: `gdcli meet notes <id>`. Read the transcript: `gdcli meet transcript <id>`. Both print a whole document with every tab. A notes document has the notes tab and, when the meeting was transcribed into the same document, a second tab with the transcript: the `document_id` of the `transcript:` line then equals that of the `smart_note:` line and both commands print the same bytes (otherwise the transcript is a separate document, or there is none: then `meet get` prints no `transcript:` line). Tab titles follow the account language: list the tabs with `gdcli docs metadata <document_id>` (`tab:` lines) and pass the id (`t.abc`) or the title to `--tab` to get only the notes or only the transcript.
4. Meet keeps a conference record only from the start of a meeting until 30 days after it, so an upcoming meeting and a meeting older than 30 days have none: take the `file_id` of the Gemini notes `attachment:` and run `gdcli docs get <file_id>`.

## Commands

| Command | Purpose | Details |
|---|---|---|
| `gdcli meet list\|get\|notes\|transcript` | meetings, artifacts, notes, transcripts | `gdcli skills meet` |
| `gdcli calendar list\|get` | events, attachments, attendees | `gdcli skills calendar` |
| `gdcli docs get\|metadata` | a Google Doc as Markdown, its tabs and owners | `gdcli skills docs` |
| `gdcli auth setup\|login\|list\|set-default` | accounts | `gdcli skills auth` |
| `gdcli skills [full\|meet\|calendar\|docs\|auth]` | prints this text, one reference, or everything (`full`) | |
| `gdcli update` | replaces gdcli with the latest release; prints `status:` (`updated` or `up_to_date`), `current:` and `latest:` | |

Ask the user before running `gdcli update` (it replaces the binary) or `gdcli auth set-default` (it changes which account every command uses). When the skill files are installed, the same details are in `references/<name>.md`.

## Output rules

- `get` commands print one record: `key: value` per scalar, `name=value` pairs for objects and list items (`attendee: name=Bee email=b@example.com response_status=accepted`), `conference: none` when a meeting has no conference record. `--json` prints the same information as one JSON object.
- List commands print a header line, then tab-separated rows. Default mode: oldest first, all `past` rows before the `upcoming` rows. Range mode (`--from`): ordered by start time only. `--json` prints an array of objects keyed by the header (`auth list` has no `--json`).
- Times are UTC (`2026-11-18T09:30:00Z`); all-day events are plain dates; Meet and Drive times can carry fractions of a second.
- Document commands (`docs get`, `meet notes`, `meet transcript`) print Markdown and have no `--json`.

## Errors

| stderr contains | Next step |
|---|---|
| ``no account is logged in: run `gdcli auth login` `` | ask the user to log in |
| `unknown account <email>; known accounts: ...` | use one of the listed accounts |
| ``no default account; run `gdcli auth set-default <email>` `` | pass `-a <email>` or ask the user to choose a default |
| ``no valid token for <email>: run `gdcli auth login` and choose that account`` | ask the user to log in again |
| `google api error 403: ...` | the account may not read this item: try another account; a disabled API (`... has not been used in project ...`) is for the user to enable |
| `google api error 404: ...` | no such id for this account: check the id, try another account |
| `no conference record for event <id>; ...` | the meeting has not started, is older than 30 days or is not visible: use the `attachment:` lines |
| `google api error 429 ...` | wait; do not retry in a loop |
