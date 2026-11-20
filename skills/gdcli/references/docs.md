# gdcli docs

`<ID>` is a document id or any URL that contains `/d/<id>` (`https://docs.google.com/document/d/<id>/edit`; the `tab=` of a pasted URL is ignored, use `--tab`).

## gdcli docs get

`gdcli docs get <ID> [--tab TAB] [-a, --account EMAIL]`

Prints the document as Markdown, exported by Google itself (no `--json`, no trailing newline added); Google limits an export to 10 MB. Without `--tab` the export holds every tab one after another (it is not the byte concatenation of the per-tab exports). `--tab` takes a tab id (`t.abc`, or `abc` without the prefix) or a title (exact, then case-insensitive) and prints that tab only (same bytes by id or by title); it uses an undocumented Google export and the Docs API, which must be enabled for the project. A per-tab export has no heading anchors, qualifies internal links with the tab and may separate checklist items by blank lines. The web export is rate limited: after about a dozen `--tab` exports within a few minutes Google answers 429.

Errors:

- `tab "<tab>" not found; available: ...` (lists every tab title and id)
- `tab "<tab>" is ambiguous: ...` (lists the candidates with their parent tabs; use the id)
- `google api error 429 ...` (the web export is throttled)

## gdcli docs metadata

`gdcli docs metadata <ID> [--json] [-a, --account EMAIL]`

Keys in order (absent when empty; times as Drive returns them, with milliseconds): `id`, `title`, `modified_time`, `created_time`, `version`, `owners` and `last_modified_by` (`name`, `email`), `html_link`, `shared`, `trashed`, `starred` (booleans), `viewed_by_me_time`, `modified_by_me_time`, `parents` (folder ids), `description`, `mime_type`, `tabs` (the flattened tab tree in document order: `id`, `title`, `index`, `parent_id` only for child tabs, `nesting_level`), `tabs_unavailable` (instead of `tabs` when the tab list failed, for example the Docs API is not enabled: Google's short message; the command still succeeds). The file must be a Google Doc.

```
id: 1AbCdEf
title: Notes by Gemini
modified_time: 2026-11-18T01:40:00.000Z
owner: name=Alice email=alice@example.com
tab: id=t.0 title=Overview index=0 nesting_level=0
tab: id=t.1 title=Details index=1 nesting_level=0
```

Errors:

- `not a google doc (mimeType <mime>)`
