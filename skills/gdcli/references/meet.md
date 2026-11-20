# gdcli meet

`<ID>` is the Calendar event id from the first column of `meet list` or `calendar list`: the only identity that exists for past and upcoming meetings (meetings started without a calendar event are not listed).

## gdcli meet list

`gdcli meet list [--past N] [--upcoming N] [--from TIME] [--to TIME] [--query TEXT] [--limit N] [--json] [-a, --account EMAIL]`

Calendar events that carry a Google Meet link (not cancelled), from the primary calendar; the Meet API is not called. Columns: `id`, `when`, `start`, `end`, `meeting_code`, `summary`. `when` is `past` (the event has ended) or `upcoming` (also a running meeting). `start` and `end` are UTC `...Z`, or a plain date for an all-day event; a missing value is an empty column; tabs and line breaks in the summary become spaces.

- Default mode: `--past N` and `--upcoming N` (alias `--future`), 0 to 1000, default 5 each, 0 skips that side. Rows: the latest N past meetings and the next N upcoming ones; all `past` rows come before the `upcoming` rows, oldest first within each.
- Range mode: `--from TIME` [`--to TIME`] [`--limit N`]: meetings that overlap `[from, to)` (`--from` is compared with the end of an event, `--to` with its start), the first N matches (1 to 1000, default 50), oldest first, ordered by start time only; exactly N rows may mean more exist. `--to` and `--limit` need `--from`; `--from` excludes `--past` and `--upcoming`.
- `--query TEXT` works in both modes: Calendar free-text search over title, description, location and people. In default mode it looks back at most 730 days (`--query standup --past 10 --upcoming 0` is the latest 10 matches within that time); use `--from` for older matches.
- `TIME` is `YYYY-MM-DD` (00:00 UTC) or RFC 3339 with an offset (`2026-11-20T00:00:00+09:00`). `--from` must be earlier than `--to` in whole seconds, else `invalid range: --from must be earlier than --to`.
- `--json` prints an array of objects with the column names as keys (all values strings), `[]` for no match.

## gdcli meet get

`gdcli meet get <ID> [--json] [-a, --account EMAIL]`

Keys in order (absent when empty): `id`, `summary`, `start`, `end`, `meeting_code`, `meeting_uri`, `access_type`, `entry_point_access`, `moderation` (the space settings; absent when Google withholds them, for example for a space the account does not own), `attachments` (`title`, `mime_type`, `file_id`), `conferences`, `meet_api`.

`conferences` has one object per conference record, oldest first (a meeting emptied and restarted has several; an empty `end` means it still runs): `id`, `start`, `end`, `participants` (`name`, `kind` `user`, `anonymous` or `phone`, `start`, `end`), `recordings` (`state`, `start`, `end`, `drive_file`), `transcripts` and `smart_notes` (`state`, `start`, `end`, `document_id`). Text prints `conference:`, `participant:`, `recording:`, `transcript:` and `smart_note:` lines; no conference record prints `conference: none`. Only the first page is read: 5 records, 250 participants, 100 of each other artifact. The `document_id` of a transcript or smart note works with `docs get` and `docs metadata`; `meet notes` and `meet transcript` print those documents when given the event id.

```
id: abc123_20261118T010000Z
summary: Weekly sync
meeting_code: abc-defg-hij
attachment: title="Notes by Gemini" mime_type=application/vnd.google-apps.document file_id=1AbCdEf
conference: id=REC1 start=2026-11-18T01:00:03Z end=2026-11-18T01:31:00Z
transcript: state=FILE_GENERATED start=2026-11-18T01:00:05Z end=2026-11-18T01:31:00Z document_id=1TrAnS
smart_note: state=FILE_GENERATED start=2026-11-18T01:00:09Z end=2026-11-18T01:31:00Z document_id=1NoTeS
```

A Meet answer 403 or 404 for one part (for example a meeting organised by someone else) does not fail the command: everything else is printed and the last line is `meet_api: unavailable (403 <message>)`.

## gdcli meet notes

`gdcli meet notes <ID> [--tab TAB] [-a, --account EMAIL]`

Prints the Gemini notes document of the meeting as Markdown (Meet `smartNotes`, state `FILE_GENERATED`; no `--json`). The document has one or two tabs, depending on the notes version and settings; tab titles are localized (in an English interface Google names them `Quick notes` and `Full notes`, but do not rely on it). Without `--tab` the whole export contains every tab (it is not the byte concatenation of the per-tab exports). List the tabs with `gdcli docs metadata <document_id>` and pass a tab id (`t.abc`) or a title to `--tab` (same bytes). One document: its Markdown only, the bytes of `docs get <document_id>`. Several documents (notes sessions of a restarted meeting): oldest first, each after a `document: id=... start=... end=...` line and followed by a blank line, so the output is then not pure Markdown. Nothing is printed when a request fails.

Errors (exit 1, empty stdout):

- `no conference record for event <id>; Meet keeps a record only from the start of a meeting until 30 days after it (see the attachment lines of calendar get)`
- `no gemini notes document for event <id>`
- `the gemini notes document of event <id> is not ready (state <state>)` (the state of the last item seen, for example `ENDED`)
- `cannot read gemini notes document <id>: google api error 403: ...` (also 404: Drive may hide a file the account cannot see)
- `google api error 403: ...` (also 404) when the conference record list or an artifact list is refused: an error here, unlike `meet get`, which prints what it can with a `meet_api` line
- `cannot read gemini notes document <id>: tab "<tab>" not found; available: ...` (also `tab "<tab>" is ambiguous: ...` after the same prefix; the same messages as `docs get --tab`)
- `event <id> has no google meet conference`

## gdcli meet transcript

`gdcli meet transcript <ID> [--tab TAB] [-a, --account EMAIL]`

Same as `meet notes` for the transcript document (Meet `transcripts`; a transcript is a separate document, not a tab).

Errors (as for `meet notes`, with `transcript` in place of `gemini notes document`):

- `no transcript for event <id>`
- `the transcript of event <id> is not ready (state <state>)`
- `cannot read transcript <id>: google api error 403: ...`
