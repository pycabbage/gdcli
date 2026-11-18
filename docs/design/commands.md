# Commands and output formats

All output is plain UTF-8 text on stdout (or JSON with `--json`), designed to be cheap for LLM agents to read. Errors go to stderr as `error: <message>` with exit status 1; usage errors exit with 2. Nothing is printed to stdout on failure.

Common rules:

- One model, two renderings. Every `get` command (and `docs metadata`) builds ONE ordered record (a JSON object, key order is part of the contract) with our own snake_case names, empty values removed. `--json` prints that record on one line; the text rendering is one generic function over the same record, so both carry exactly the same information (the tests compare every text line with the record, including `false` values). Text rules: a top-level scalar is `key: value`; an array of scalars prints one `key: value` line per element with the singular key (the array key without its trailing `s`: `recurrence` stays, `attendees` gives `attendee:`); an array element that is an object, and a nested object value (`organizer`), print ONE line `key: name=value name=value ...` with every scalar leaf as a `name=value` pair in field order, using exactly the JSON field names, so no value is ever positional; a value is bare unless it is empty or contains whitespace, `"` or `=`, in which case it is JSON-quoted (`name="Dr. Bee"`); numbers and booleans print as they are (`index=0`, `optional=false`; `false` is never dropped); arrays nested inside an array element print after its line, with their own singular keys; records have at most one level of nesting inside array elements and objects, and an object nested deeper (which no record produces today) is never dropped but printed as one quoted JSON string value (`inner="{\"b\":\"x y\"}"`); an empty array prints `key: none` (singular key); multi-line top-level scalars (CR and CRLF are normalised to LF) continue on the following lines indented by two spaces; null and empty values never appear.
- List commands (`calendar list`, `meet list`) print a header line first (always, also when there are no items; a static string, the same number of tab-separated columns as every row), then one tab-separated line per item, chronological ascending (oldest first), past items before upcoming items. Tabs, CR and LF inside values are replaced by a space. Missing values are empty columns (a row may end with a tab when its last column is empty). If the API calls fail, nothing is printed, not even the header. With `--json` the output is one JSON array of objects whose keys are exactly the header column names (all values strings), same rows, same order; no items print `[]`.
- JSON is compact (no pretty printing) and ends with one newline.
- `id` is the FIRST column of every list row, it is always the Calendar event id (the occurrence id returned with `singleEvents=true`: unique per occurrence and in the same format for past and upcoming rows), and it is the argument of both `gdcli calendar get <id>` and `gdcli meet get <id>`. Both list commands read only the Calendar API; the Meet REST API is used only by `meet get`.
- Why the Calendar event is the identity: Meet has no object for future meetings, and a conference record carries no link to the calendar event or occurrence it was started from. The Calendar event is therefore the only identity that exists for both past and upcoming meetings. Known limitation: ad-hoc meetings that were started without a calendar event are not listed.
- Timestamps: timed events are printed as RFC 3339 in UTC with second precision (`2026-11-18T09:30:00Z`) in lists and in `calendar get`/`meet get`; all-day events as their plain local calendar date (`2026-11-20`, not a UTC instant); Meet timestamps of conference records, participants and artifacts and the Drive times of `docs metadata` are passed through exactly as the API returns them. `docs get` prints Google's export bytes unchanged (no trailing newline is added).
- A closed output pipe (`gdcli calendar list | head -1`) ends the command quietly with exit status 0 and no message. Only an I/O error of kind `BrokenPipe` raised directly by writing the output counts; a broken pipe inside a network error is a normal error.
- ID arguments: `calendar get` and `meet get` take an event id; clap rejects an empty id, `.`, `..` and any id containing `/ ? # %` with `invalid event id` (exit status 2) before any request is built. `docs get` and `docs metadata` take a document id or docs URL; an empty id, `.` and `..` are rejected with `invalid document id` (exit status 2).
- `--json` exists on exactly `calendar list`, `calendar get`, `meet list`, `meet get` and `docs metadata`. It does not exist on `docs get`: the option is allowed only when no official Markdown API exists, and Drive `files.export` is the official Markdown API.
- Timeouts: connecting to a Google host fails after 10 s, and an API command (everything except `setup` and `login`, which wait for the user) fails with `error: timed out after 120s` when it has not finished after 120 s.
- `fields` partial responses are sent on every JSON request, so only the printed data is transferred.

Error text: every message is built from our own lowercase context (`cannot ...`, `token retrieval failed: ...`, `google api error ...`, `request failed: ...`), so the first character is lowercase by construction and no formatting helper exists; a binary-level test checks the real stderr. Google API errors print as `google api error <status>: <message>` when the response carries a JSON `error.message` (cut at 300 characters, `...` appended), and as `google api error <status> <reason>` (for example `google api error 429 Too Many Requests`) otherwise, for Calendar, Drive, Docs and Meet alike. A successful answer that is not JSON prints `unexpected google api response: <parse error>`. Raw response bodies, headers and debug dumps are never printed.

## `gdcli setup`

Prints `docs/setup.md`, `Destination: <path>`, and the prompt `Path of the downloaded JSON (empty if already placed at the destination): `. The typed path is trimmed, surrounding quotes are removed and a leading `~/` is expanded to the home directory. Then installs and validates the client secret and runs the login flow (see `auth.md`).

## `gdcli login`

Runs the OAuth login flow. Prints the authorization URL, then `logged in; token stored at <path>`.

## `gdcli calendar list [--past N] [--upcoming N] [--json]`

Primary calendar. `--past N` and `--upcoming N` (alias `--future`) choose how many items of each side are fetched: default 5 each, accepted range 0 to 1000 (clap rejects the rest with exit status 2). `0` skips that side entirely: no request is made for it. Memory stays bounded by N (a ring buffer of N for the past side, N collected events for the upcoming side). The first line is the header `id<TAB>when<TAB>start<TAB>end<TAB>summary` (`events::CALENDAR_HEADER`, the only copy of the text); the JSON keys are the same five names.

| Column | Meaning | Values | Empty when |
|---|---|---|---|
| `id` | event id, the argument of `calendar get` and `meet get` | Calendar event id | never |
| `when` | which group the event belongs to | `past`, `upcoming` | never |
| `start` | start of the event | RFC 3339 UTC, or `YYYY-MM-DD` for all-day events | the API sent no start |
| `end` | end of the event (exclusive date for all-day events) | RFC 3339 UTC or `YYYY-MM-DD` | the API sent no end |
| `summary` | event title | text, tabs and line breaks replaced by spaces | the event has no title |

Selection (the two lookups run concurrently; one implementation, `events::list`, shared with `meet list` and parameterised by a predicate):

- Every `events.list` request carries `eventTypes=default&eventTypes=focusTime&eventTypes=outOfOffice&eventTypes=fromGmail` (the parameter is repeated). Working-location and birthday events are not appointments and would drown the list (for a real account about half of the all-day entries were working locations).
- Upcoming: `singleEvents=true`, `orderBy=startTime`, `timeMin=now`, `maxResults=min(N, 250)`; pages continue until N non-cancelled events that match the predicate are collected or the calendar is exhausted. `timeMin` filters on the event end, so an event that is running now is an upcoming event.
- Past: the API cannot sort descending, so the newest past events are found by searching backwards over nested windows `[now - w, now)` for `w` = 7, 30, 180 and 730 days (`timeMax=now`, `orderBy=startTime`, `maxResults=250`). Within a window all pages are read, the predicate is applied first, and only the last N events that have ended (`end <= now`, not cancelled) are kept in a ring buffer. The search stops at the first window that yields N events; if even the 730 day window has fewer, those are printed.
- An event is in exactly one group: `end <= now` is past, otherwise upcoming; both groups drop events on the other side of that line (so a server answer that disagrees with the client clock cannot print an event twice) and cancelled events. All-day events are treated as ending at 00:00 UTC of their exclusive end date, while the server filters `timeMin`/`timeMax` in the calendar's time zone, so in time zones ahead of UTC an all-day event can be absent from both lists for a few hours around its end. The time zone is not requested to correct this.

## `gdcli calendar get <id> [--json]`

`events.get` on the primary calendar with a `fields` mask. The record, in this order (keys absent when empty):

| Key | Type | Meaning |
|---|---|---|
| `id`, `summary`, `status` | string | event id, title, `confirmed` / `tentative` / `cancelled` |
| `start`, `end` | string | UTC `...Z` for timed events, plain date for all-day events |
| `time_zone` | string | time zone name of the start |
| `location` | string | |
| `meeting_code` | string | Google Meet code when the event has a Meet conference |
| `conference_id` | string | conference id of a non-Meet conference (Zoom, ...), only when `meeting_code` is absent |
| `meeting_uri` | string | the `hangoutLink`, else `https://meet.google.com/<meeting_code>` |
| `html_link` | string | link to the event in Calendar |
| `organizer`, `creator` | object `{name, email}` | |
| `attendees` | array of `{name, email, response_status, optional}` | `optional` only when true |
| `recurrence` | array of string | RRULE / EXDATE lines |
| `event_type`, `visibility`, `recurring_event_id` | string | |
| `entry_points` | array of `{type, uri}` | video, phone, ... |
| `created`, `updated` | string | UTC `...Z` |
| `description` | string | last; may span lines |

Text example: `id: abc`, `summary: Planning`, ..., `organizer: name=Boss email=boss@example.com`, `attendee: name=Bee email=b@example.com response_status=needsAction optional=true`, `entry_point: type=video uri=https://meet.google.com/abc-defg-hij`, `description: First line` (continuation lines indented by two spaces).

## `gdcli meet list [--past N] [--upcoming N] [--json]`

The first line is the header `id<TAB>when<TAB>start<TAB>end<TAB>meeting_code<TAB>summary` (`events::MEET_HEADER`; the JSON keys are the same six names). It is `calendar list` restricted to events that carry a Google Meet link (`conferenceData.conferenceSolution.key.type == hangoutsMeet`, not cancelled), with one extra column. The count options work as in `calendar list`; for Meet the upcoming page size stays 25.

| Column | Meaning | Values | Empty when |
|---|---|---|---|
| `id` | event id, the argument of `meet get` and `calendar get` | Calendar event id | never |
| `when` | which group the meeting belongs to | `past`, `upcoming` | never |
| `start` | start of the event | RFC 3339 UTC, or `YYYY-MM-DD` for all-day events | the API sent no start |
| `end` | end of the event | same formats as `start` | the API sent no end |
| `meeting_code` | `conferenceData.conferenceId`, the Meet code | `abc-defg-hij` | never (rows without a Meet link are not listed) |
| `summary` | event title | text, tabs and line breaks replaced by spaces | the event has no title |

Same selection rules as `calendar list` (past = ended, a running meeting counts as upcoming, cancelled events dropped, no duplicates, nested past windows with the predicate applied before the N element ring buffer). The `fields` mask adds `conferenceData(conferenceId,conferenceSolution(key(type)))` to the calendar mask. The Meet REST API is not called, so the list needs no Meet scope or Meet API access.

## `gdcli meet get <id> [--json]`

`<id>` is the event id printed by `meet list` or `calendar list`; nothing else is accepted (no meeting codes, no conference record ids). Flow:

1. `events.get` of the event (`fields=id,summary,start,end,conferenceData(...)`). If it has no Google Meet conference the command fails with `event <id> has no google meet conference`.
2. Concurrently: `spaces.get` of `spaces/<meeting code>` and `conferenceRecords.list` (`pageSize=5`, newest first) with the filter `space.meeting_code = "<code>" AND start_time>="<event start - 1h>" AND start_time<="<event end + 1h>"` (RFC 3339 UTC with milliseconds; the Meet filter grammar documents `AND` and `start_time` comparisons). A meeting code is shared by every occurrence of a recurring event, so the time window selects the occurrence; the slack is the constant `SLACK_HOURS`. All-day events (and events without date-times) use only the meeting code.
3. For each returned conference record (typically one), concurrently: participants (`pageSize=250`), recordings and transcripts (`pageSize=100`).

Record (keys absent when empty):

| Key | Type | Meaning |
|---|---|---|
| `id`, `summary`, `start`, `end` | string | event fields, times as in `calendar get` |
| `meeting_code`, `meeting_uri` | string | the code and `https://meet.google.com/<code>` |
| `access_type`, `entry_point_access`, `moderation` | string | from the Meet space `config` (Google withholds it for spaces the account does not own, then the keys are absent) |
| `conferences` | array, oldest first | one object per conference record; present (possibly empty) when the record list succeeded, absent when that call failed |
| `meet_api` | string | `unavailable (<code> <google message>)`, only when a Meet call answered 403/404 |

A conference object has `id`, `start`, `end` (as the API returns them; an empty end means it is still running), and the arrays `participants` (`{name, kind, start, end}` with `kind` `user`, `anonymous` or `phone`), `recordings` (`{state, start, end, drive_file}`) and `transcripts` (`{state, start, end, document_id}`; feed `document_id` to `docs get`). In text each conference prints `conference: id=REC1 start=... end=...` followed by `participant: name=Alice kind=user start=... end=...`, `recording: state=FILE_GENERATED start=... end=... drive_file=...` and `transcript: state=ENDED document_id=...` lines; an empty `conferences` array prints `conference: none`.

Meet 403 or 404 answers are tolerated per call (space, conference list, participants, recordings, transcripts, for example a meeting organised by someone else): everything that did arrive is printed, followed by the `meet_api` entry (the first failure), and the exit status is 0. Every other error is a normal error and nothing is printed.

## `gdcli docs get <id> [--tab <id|title>]`

`<id>` is a document id or any URL containing `/d/<id>` (for example `https://docs.google.com/document/d/<id>/edit?tab=t.0`; the `tab=` of a pasted URL is ignored, use `--tab`). There is no `--json`: the document is exported as Markdown by Google itself, which is the official Markdown API, so no conversion is implemented.

- Without `--tab`: Drive `files.export` with `mimeType=text/markdown`, the whole document; the tabs of a multi-tab document are concatenated with nothing between them (no tab titles, no separator). The response body is copied to stdout chunk by chunk as it arrives. Google limits exported content to 10 MB; larger documents fail with the API's `exportSizeLimitExceeded` message.
- With `--tab <TAB>`: one tab only. No official API exports a single tab as Markdown (Drive `files.export` has only `mimeType`; a `tab=` parameter there is silently ignored). gdcli therefore (1) lists the tabs with the Docs API, (2) resolves `<TAB>` client-side, and (3) requests the Docs web export `https://docs.google.com/document/d/<id>/export?format=md&tab=<tabId>`. This is an explicit exception, pending owner sign-off, because the `tab=` export parameter is undocumented and could change; exit plan: own conversion from the Docs API JSON (about 300 lines).
  1. Tab list: `GET docs.googleapis.com/v1/documents/<id>?includeTabsContent=true&fields=tabs(P,childTabs(P,childTabs(P)))` with `P = tabProperties(tabId,title,index,parentTabId,nestingLevel)`; child tabs up to three levels are flattened in document order. Needs the Google Docs API enabled in the project (see `setup.md`); the drive.readonly token is accepted.
  2. Resolution, always client-side because Google silently returns the first tab for an unknown or unprefixed `tab=`: (a) exact tab id; (b) when the argument lacks the `t.` prefix, exact tab id `t.<argument>`; (c) exact title (case-sensitive); (d) case-insensitive title. The first step with exactly one match wins. More than one match in a step is an error that lists the candidates and never picks one (`tab "Notes" is ambiguous: t.bbb (Notes), t.ccc (Overview > Notes)`; the path shows the parent tabs); no match is `tab "x" not found; available: "A" (t.aaa), "B" (t.bbb)`. There is no `Parent/Child` syntax. The full resolved tab id is always sent.
  3. Export (observed live on a real account): the request carries `Authorization: Bearer`; Google answers 307 to a `https://doc-xx-docstext.googleusercontent.com/export/...` URL that is fetched WITHOUT the Authorization header. At most one redirect is followed, only to an `https` URL whose host ends with `.googleusercontent.com` (anything else, for example a login page, is `export redirected to an unexpected host`; the bearer token is never sent to the redirected host). The final response must have a content type starting with `text/markdown` or `text/x-markdown`, compared case-insensitively (else `unexpected export content type`, for example for `text/html`) and is streamed to stdout. The web export is rate limited: docs.google.com answers 429 (HTML) after roughly a dozen exports within a few minutes; the CLI prints `google api error 429 Too Many Requests` and does not retry.
  4. Observed facts (live check with a real account, 2026-11-18): the 307 target is `https://doc-xx-xx-docstext.googleusercontent.com/export/...?format=md&tab=...&dat=...` (host class `*.googleusercontent.com`); the final answer is `200` with `Content-Type: text/x-markdown; charset=utf-8` (not `text/markdown`; the body is valid Markdown) and a `Content-Disposition` header that carries the document title, which gdcli ignores. Per-tab Markdown differs from the whole-document export only in formatting: headings carry no `{#id}` anchors, internal links are tab-qualified (`(?tab=t.xxx#heading=h.xxx)`) instead of `(#MM:SS)`, and checkbox lists are loose (blank lines between items); the content is otherwise identical. Neither export ends with a newline. The whole-document export is the concatenation of the tab exports with nothing between them.
  5. `--tab` must not be blank: clap rejects it with `invalid tab: it must not be blank` (exit status 2) before any request.

## `gdcli docs metadata <id> [--json]`

Only the metadata of a Google Doc. Two calls run concurrently: Drive `files.get` (`supportsAllDrives=true`, `fields=id,name,mimeType,createdTime,modifiedTime,version,webViewLink,owners(displayName,emailAddress),lastModifyingUser(displayName,emailAddress),shared,trashed,starred,viewedByMeTime,modifiedByMeTime,parents,description`) and the Docs API tab list of `docs get --tab`. The file must be a Google Doc (`mimeType` `application/vnd.google-apps.document`), else `not a google doc (mimeType <x>)`. Record, in this order (keys absent when empty; times as Drive returns them):

| Key | Type | Meaning |
|---|---|---|
| `id` | string | document id |
| `title` | string | Drive `name` |
| `modified_time`, `created_time` | string | RFC 3339 UTC |
| `version` | string | Drive version number |
| `owners` | array of `{name, email}` | |
| `last_modified_by` | object `{name, email}` | |
| `html_link` | string | `webViewLink` |
| `shared`, `trashed`, `starred` | boolean | |
| `viewed_by_me_time`, `modified_by_me_time` | string | |
| `parents` | array of string | folder ids |
| `description` | string | |
| `mime_type` | string | |
| `tabs` | array of `{id, title, index, parent_id, nesting_level}` | flattened tab tree; `parent_id` only for child tabs; text `tab: id=t.first title=Overview index=0 nesting_level=0` |
| `tabs_unavailable` | string | instead of `tabs` when the Docs API call fails (for example the API is not enabled): Google's short message; the command still succeeds |
