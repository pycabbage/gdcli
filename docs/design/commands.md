# Commands and output formats

All output is plain UTF-8 text on stdout, designed to be cheap for LLM agents to read. Errors go to stderr as `error: <message>` with exit status 1; usage errors exit with 2. Nothing is printed to stdout on failure.

Common rules:

- List commands print one tab-separated line per item, no header, chronological ascending (oldest first), past items before upcoming items. Tabs, CR and LF inside values are replaced by a space. Missing values are empty columns.
- `get` commands print `key: value` lines in a fixed order and omit empty fields. Values that contain line breaks continue on following lines indented by two spaces. Repeated keys (`attendee`, `participant`, ...) appear once per item.
- Timestamps are RFC 3339 in UTC with second precision (`2026-11-18T09:30:00Z`) for Calendar data; Meet timestamps are passed through exactly as the API returns them. All-day events print the plain date (`2026-11-20`).
- `fields` partial responses are always sent, so only the printed data is transferred.

Google API errors are reduced to `google api error <code>: <message>` when the response carries a JSON error body.

## `gdcli setup`

Prints `docs/setup.md`, `Destination: <path>`, and the prompt `Path of the downloaded JSON (empty if already placed at the destination): `. The typed path is trimmed, surrounding quotes are removed and a leading `~/` is expanded to the home directory. Then installs and validates the client secret and runs the login flow (see `auth.md`).

## `gdcli login`

Runs the OAuth login flow. Prints the authorization URL, then `logged in; token stored at <path>`.

## `gdcli calendar list`

Primary calendar. Columns:

1. `past` or `upcoming`
2. event id (usable with `calendar get`)
3. start (RFC 3339 or date)
4. end (RFC 3339 or date)
5. summary

Selection (at most 5 + 5 lines; the two lookups run concurrently):

- Upcoming: `events.list` with `singleEvents=true`, `orderBy=startTime`, `timeMin=now`, `maxResults=5`; pages continue until 5 non-cancelled events are collected or the calendar is exhausted. `timeMin` filters on the event end, so an event that is running now is an upcoming event.
- Past: the API cannot sort descending, so the newest past events are found by searching backwards over nested windows `[now - w, now)` for `w` = 7, 30, 180 and 730 days (`timeMax=now`, `orderBy=startTime`, `maxResults=250`). Within a window all pages are read but only the last 5 events that have ended (`end <= now`, not cancelled) are kept in a ring buffer, so memory stays bounded whatever the window holds. The search stops at the first window that yields 5 events; if even the 730 day window has fewer, those are printed.
- An event is in exactly one group: `end <= now` is past, otherwise upcoming; both groups drop events on the other side of that line (so a server answer that disagrees with the client clock cannot print an event twice) and cancelled events. All-day events are treated as ending at 00:00 UTC of their exclusive end date, while the server filters `timeMin`/`timeMax` in the calendar's time zone, so in time zones ahead of UTC an all-day event can be absent from both lists for a few hours around its end. The time zone is not requested to correct this.

## `gdcli calendar get <id>`

`events.get` on the primary calendar. Lines (when present): `id`, `status`, `summary`, `start`, `end`, `time_zone` (of the start), `location`, `event_type`, `recurring_event_id`, `recurrence` (per rule), `visibility`, `organizer` (`Name <email>`), `creator`, `attendee` (`Name <email> [responseStatus]`, per attendee), `html_link`, `meet_link`, `conference_id`, `entry_point_<type>` (per entry point), `created`, `updated`, `description` (last, may span lines).

## `gdcli meet list`

Columns: kind, id, start, end, detail.

- Past (5): Meet REST `conferenceRecords.list` with `pageSize=5` (the API default order is start time descending, reversed for output). `id` is the conference record id (without `conferenceRecords/`), `detail` is the space resource name (`spaces/<id>`). An empty `end` means the conference has not ended.
- Upcoming (5): the Meet REST API has no scheduled meetings, so upcoming meetings come from Calendar. Events with `timeMin=now`, ascending, 25 per page, filtered client side to non-cancelled events whose `conferenceData.conferenceSolution.key.type` is `hangoutsMeet`; pages continue until 5 are found. `id` is `conferenceData.conferenceId`, which is the meeting code (`abc-defg-hij`), `detail` is the event summary.
- Both lookups run concurrently.
- Known limitation: a meeting that is running right now can appear twice, once as `past` (conference record without end) and once as `upcoming` (its calendar event is still running). The record only carries the space id, not the meeting code, so matching them would need an extra `spaces.get` call per running record; this is deliberately not done.

## `gdcli meet get <id>`

`<id>` is classified by `^[a-z]{3}-[a-z]{4}-[a-z]{3}$` (an optional `https://meet.google.com/` prefix is stripped and the input is lowercased before matching, since meeting codes are case-insensitive):

- Meeting code: `spaces.get` and the 5 most recent `conferenceRecords.list` entries filtered by `space.meeting_code = "<code>"`, concurrently. Lines: `id`, `space`, `meeting_code`, `meeting_uri`, `access_type`, `entry_point_access`, `moderation`, `active_conference` (record id of a running conference), then one `conference: <record id> <start> [<end>]` per recent conference, oldest first.
- Anything else is a conference record id, with or without the `conferenceRecords/` prefix: `conferenceRecords.get` followed by `spaces.get` of its space (one chain), concurrently with participants, recordings and transcripts. Lines: `id`, `start`, `end`, then the space lines above, then
  - `participant: <name> (<user|anonymous|phone>) <earliest start> <latest end>`
  - `recording: <state> <start> <end> drive:<Drive file id>`
  - `transcript: <state> <start> <end> doc:<Google Docs document id>` (feed this id to `docs get`)

Meeting codes can expire (about 365 days after last use) and be reused; a past meeting should be addressed by its conference record id.

## `gdcli docs get <id>`

`<id>` is a document id or any URL containing `/d/<id>` (for example `https://docs.google.com/document/d/<id>/edit?tab=t.0`). The document is exported with Drive `files.export` and `mimeType=text/markdown`, which Google documents as a supported export format for Google Docs, so no Markdown conversion is implemented and there is no `--json` option (the owner's rule: the option exists only when no official Markdown API exists). The response body is copied to stdout chunk by chunk as it arrives. Google limits exported content to 10 MB; larger documents fail with the API's `exportSizeLimitExceeded` message.
