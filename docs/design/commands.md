# Commands and output formats

All output is plain UTF-8 text on stdout, designed to be cheap for LLM agents to read. Errors go to stderr as `error: <message>` with exit status 1; usage errors exit with 2. Nothing is printed to stdout on failure.

Common rules:

- List commands print a header line first (always, also when there are no items; a static string, the same number of tab-separated columns as every row), then one tab-separated line per item, chronological ascending (oldest first), past items before upcoming items. Tabs, CR and LF inside values are replaced by a space. Missing values are empty columns (a row may end with a tab when its last column is empty). If the API calls fail, nothing is printed, not even the header.
- `id` is the FIRST column of every list row, it is always the Calendar event id (the occurrence id returned with `singleEvents=true`: unique per occurrence and in the same format for past and upcoming rows), and it is the argument of both `gdcli calendar get <id>` and `gdcli meet get <id>`. Both list commands read only the Calendar API; the Meet REST API is used only by `meet get`.
- Why the Calendar event is the identity: Meet has no object for future meetings, and a conference record carries no link to the calendar event or occurrence it was started from. The Calendar event is therefore the only identity that exists for both past and upcoming meetings. Known limitation: ad-hoc meetings that were started without a calendar event are not listed.
- `get` commands print `key: value` lines in a fixed order and omit empty fields. Values that contain line breaks continue on following lines indented by two spaces. Repeated keys (`attendee`, `participant`, ...) appear once per item.
- Timestamps: list rows print timed events as RFC 3339 in UTC with second precision (`2026-11-18T09:30:00Z`) and all-day events as their plain local calendar date (`2026-11-20`); `calendar get` and `docs get` print what Google returns, unchanged; Meet timestamps in `meet get` are passed through exactly as the API returns them.
- A closed output pipe (`gdcli calendar list | head -1`) ends the command quietly with exit status 0 and no message. Only an I/O error of kind `BrokenPipe` raised directly by writing the output counts; a broken pipe inside a network error is a normal error.
- ID arguments: `calendar get` and `meet get` take an event id; clap rejects an empty id, `.`, `..` and any id containing `/ ? # %` with `invalid event id` (exit status 2) before any request is built. `docs get` takes a non-empty document id or URL.
- Timeouts: connecting to a Google host fails after 10 s, and an API command (everything except `setup` and `login`, which wait for the user) fails with `error: timed out after 120s` when it has not finished after 120 s.
- `fields` partial responses are always sent, so only the printed data is transferred.

Error text: every message is built from our own lowercase context (`cannot ...`, `token retrieval failed: ...`, `google api error ...`, `request failed: ...`), so the first character is lowercase by construction and no formatting helper exists; a binary-level test checks the real stderr. Google API errors print as `google api error <status>: <message>` when the response carries a JSON `error.message`, and as `google api error <status> <reason>` (for example `google api error 404 Not Found`) otherwise, for Calendar, Drive and Meet alike. A successful answer that is not JSON prints `unexpected google api response: <parse error>`. Raw response bodies, headers and debug dumps are never printed.

## `gdcli setup`

Prints `docs/setup.md`, `Destination: <path>`, and the prompt `Path of the downloaded JSON (empty if already placed at the destination): `. The typed path is trimmed, surrounding quotes are removed and a leading `~/` is expanded to the home directory. Then installs and validates the client secret and runs the login flow (see `auth.md`).

## `gdcli login`

Runs the OAuth login flow. Prints the authorization URL, then `logged in; token stored at <path>`.

## `gdcli calendar list`

Primary calendar. The first line is the header `id<TAB>when<TAB>start<TAB>end<TAB>summary` (`events::CALENDAR_HEADER`, the only copy of the text).

| Column | Meaning | Values | Empty when |
|---|---|---|---|
| `id` | event id, the argument of `calendar get` and `meet get` | Calendar event id | never |
| `when` | which group the event belongs to | `past`, `upcoming` | never |
| `start` | start of the event | RFC 3339 UTC, or `YYYY-MM-DD` for all-day events | the API sent no start |
| `end` | end of the event (exclusive date for all-day events) | RFC 3339 UTC or `YYYY-MM-DD` | the API sent no end |
| `summary` | event title | text, tabs and line breaks replaced by spaces | the event has no title |

Selection (at most 5 + 5 rows; the two lookups run concurrently; one implementation, `events::list`, shared with `meet list` and parameterised by a predicate):

- Every `events.list` request carries `eventTypes=default&eventTypes=focusTime&eventTypes=outOfOffice&eventTypes=fromGmail` (the parameter is repeated). Working-location and birthday events are not appointments and would drown the list (for a real account about half of the all-day entries were working locations).
- Upcoming: `singleEvents=true`, `orderBy=startTime`, `timeMin=now`, `maxResults=5`; pages continue until 5 non-cancelled events that match the predicate are collected or the calendar is exhausted. `timeMin` filters on the event end, so an event that is running now is an upcoming event.
- Past: the API cannot sort descending, so the newest past events are found by searching backwards over nested windows `[now - w, now)` for `w` = 7, 30, 180 and 730 days (`timeMax=now`, `orderBy=startTime`, `maxResults=250`). Within a window all pages are read, the predicate is applied first, and only the last 5 events that have ended (`end <= now`, not cancelled) are kept in a ring buffer, so memory stays bounded whatever the window holds. The search stops at the first window that yields 5 events; if even the 730 day window has fewer, those are printed.
- An event is in exactly one group: `end <= now` is past, otherwise upcoming; both groups drop events on the other side of that line (so a server answer that disagrees with the client clock cannot print an event twice) and cancelled events. All-day events are treated as ending at 00:00 UTC of their exclusive end date, while the server filters `timeMin`/`timeMax` in the calendar's time zone, so in time zones ahead of UTC an all-day event can be absent from both lists for a few hours around its end. The time zone is not requested to correct this.

## `gdcli calendar get <id>`

`events.get` on the primary calendar with a `fields` mask (`id`, `status`, `summary`, `description`, `location`, `start`, `end`, `htmlLink`, `hangoutLink`, `eventType`, `recurringEventId`, `recurrence`, `visibility`, `created`, `updated`, `organizer`, `creator`, `attendees` and `conferenceData.conferenceId` / `entryPoints`). The answer is printed by `output::flatten`: every non-empty scalar becomes one `key: value` line, nested keys are joined with `.`, array items get their index (`attendees.0.email`), keys appear in alphabetical order, multi-line values (the description) continue on lines indented by two spaces, and timestamps are printed exactly as Google returns them (with the event's UTC offset; the time zone name is `start.timeZone`).

## `gdcli meet list`

The first line is the header `id<TAB>when<TAB>start<TAB>end<TAB>meeting_code<TAB>summary` (`events::MEET_HEADER`). It is `calendar list` restricted to events that carry a Google Meet link (`conferenceData.conferenceSolution.key.type == hangoutsMeet`, not cancelled), with one extra column.

| Column | Meaning | Values | Empty when |
|---|---|---|---|
| `id` | event id, the argument of `meet get` and `calendar get` | Calendar event id | never |
| `when` | which group the meeting belongs to | `past`, `upcoming` | never |
| `start` | start of the event | RFC 3339 UTC, or `YYYY-MM-DD` for all-day events | the API sent no start |
| `end` | end of the event | same formats as `start` | the API sent no end |
| `meeting_code` | `conferenceData.conferenceId`, the Meet code | `abc-defg-hij` | never (rows without a Meet link are not listed) |
| `summary` | event title | text, tabs and line breaks replaced by spaces | the event has no title |

Same selection rules as `calendar list` (past = ended, a running meeting counts as upcoming, cancelled events dropped, no duplicates, nested past windows with the predicate applied before the 5 element ring buffer, upcoming pages of 25 until 5 matching events are found). The `fields` mask adds `conferenceData(conferenceId,conferenceSolution(key(type)))` to the calendar mask. The Meet REST API is not called, so the list needs no Meet scope or Meet API access.

## `gdcli meet get <id>`

`<id>` is the event id printed by `meet list` or `calendar list`; nothing else is accepted (no meeting codes, no conference record ids). Flow:

1. `events.get` of the event (`fields=id,summary,start,end,conferenceData(...)`). If it has no Google Meet conference the command fails with `event <id> has no google meet conference`.
2. Concurrently: `spaces.get` of `spaces/<meeting code>` and `conferenceRecords.list` (`pageSize=5`, newest first) with the filter `space.meeting_code = "<code>" AND start_time>="<event start - 1h>" AND start_time<="<event end + 1h>"` (RFC 3339 UTC with milliseconds; the Meet filter grammar documents `AND` and `start_time` comparisons). A meeting code is shared by every occurrence of a recurring event, so the time window selects the occurrence; the slack is the constant `SLACK_HOURS`. All-day events (and events without date-times) use only the meeting code.
3. For each returned conference record (typically one), concurrently: participants (`pageSize=250`), recordings and transcripts (`pageSize=100`).

Output lines: `id`, `summary`, `start`, `end`, `meeting_code`, `meeting_uri` (`https://meet.google.com/<code>`, derived from the code), then `access_type`, `entry_point_access`, `moderation` from the space, then per conference record, oldest first, a block:

- `conference: <record id> <start> [<end>]` (an empty end means it is still running)
- `participant: <name> (<user|anonymous|phone>) <earliest start> <latest end>`
- `recording: <state> <start> <end> drive:<Drive file id>`
- `transcript: <state> <start> <end> doc:<Google Docs document id>` (feed this id to `docs get`)

When the lookup succeeded but no conference record matches the occurrence (it has not happened, or Google keeps no record), the single line `conference: none` is printed instead of the blocks.

Meet 403 or 404 answers are tolerated per call (space, conference list, participants, recordings, transcripts, for example a meeting organised by someone else): everything that did arrive is printed, followed by the single line `meet_api: unavailable (<code> <google message>)` (the first failure), and the exit status is 0; no `conference: none` line is printed when a call failed because absence cannot be told from a denied call. Every other error is a normal error and nothing is printed. Observed on a real account: for spaces the account does not own, Google withholds the space `config`, so the `access_type`, `entry_point_access` and `moderation` lines are legitimately absent.

## `gdcli docs get <id>`

`<id>` is a document id or any URL containing `/d/<id>` (for example `https://docs.google.com/document/d/<id>/edit?tab=t.0`). The document is exported with Drive `files.export` and `mimeType=text/markdown`, which Google documents as a supported export format for Google Docs, so no Markdown conversion is implemented and there is no `--json` option (the owner's rule: the option exists only when no official Markdown API exists). The response body is copied to stdout chunk by chunk as it arrives. Google limits exported content to 10 MB; larger documents fail with the API's `exportSizeLimitExceeded` message. The output is Google's export bytes unchanged: gdcli adds no trailing newline and does not validate the encoding.
