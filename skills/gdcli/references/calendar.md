# gdcli calendar

Events of the primary calendar of the account. `<ID>` is the event id from the first column of `calendar list` or `meet list` (one id per occurrence of a recurring event).

## gdcli calendar list

`gdcli calendar list [--past N] [--upcoming N] [--from TIME] [--to TIME] [--query TEXT] [--limit N] [--json] [-a, --account EMAIL]`

Columns: `id`, `when`, `start`, `end`, `summary`. `when` is `past` (the event has ended) or `upcoming` (also a running event). `start` and `end` are UTC `...Z`, or a plain date for an all-day event (`end` exclusive); a missing value is an empty column; tabs and line breaks in the summary become spaces. Listed: normal, focus-time, out-of-office and Gmail events; not listed: cancelled events, working locations, birthdays.

- Default mode: `--past N` and `--upcoming N` (alias `--future`), 0 to 1000, default 5 each, 0 skips that side. Rows: the latest N past events and the next N upcoming ones; all `past` rows come before the `upcoming` rows, oldest first within each.
- Range mode: `--from TIME` [`--to TIME`] [`--limit N`]: events that overlap `[from, to)` (`--from` is compared with the end of an event, `--to` with its start), the first N matches (1 to 1000, default 50), oldest first, ordered by start time only; exactly N rows may mean more exist. `--to` and `--limit` need `--from`; `--from` excludes `--past` and `--upcoming`.
- `--query TEXT` works in both modes: Calendar free-text search over title, description, location and people. In default mode it looks back at most 730 days; use `--from` for older matches.
- `TIME` is `YYYY-MM-DD` (00:00 UTC) or RFC 3339 with an offset (`2026-11-20T00:00:00+09:00`). `--from` must be earlier than `--to` in whole seconds, else `invalid range: --from must be earlier than --to`.
- `--json` prints an array of objects with the column names as keys (all values strings), `[]` for no match.

## gdcli calendar get

`gdcli calendar get <ID> [--json] [-a, --account EMAIL]`

Keys in order (absent when empty): `id`, `summary`, `status` (`confirmed`, `tentative` or `cancelled`), `start`, `end` (UTC `...Z`, or a plain date for all-day events), `time_zone`, `location`, `meeting_code` (Google Meet code), `conference_id` (a non-Meet conference such as Zoom, only without `meeting_code`), `meeting_uri`, `html_link`, `organizer` and `creator` (`name`, `email`), `attendees` (`name`, `email`, `response_status`, `optional` only when true), `recurrence` (RRULE and EXDATE lines), `event_type`, `visibility`, `recurring_event_id`, `entry_points` (`type`, `uri`), `attachments` (`title`, `mime_type`, `file_id`; `file_id` is absent for an attachment without one), `created`, `updated`, `description` (last; continuation lines are indented by two spaces). Lists print as `attendee:`, `entry_point:` and `attachment:` lines.

```
attendee: name=Bee email=b@example.com response_status=needsAction optional=true
attachment: title="Notes by Gemini" mime_type=application/vnd.google-apps.document file_id=1AbCdEf
```

`file_id` is a Drive id: `gdcli docs get <file_id>` prints a Google Doc attachment as Markdown.
