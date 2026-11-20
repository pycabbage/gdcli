# Accounts

gdcli keeps one OAuth token per Google account. Code: `src/auth/accounts.rs` (storage, selection, migration), `src/auth/token.rs` (authenticator and login flow), `src/auth/mod.rs` (`auth list`, `auth set-default`). Authentication facts that do not depend on accounts (scopes, flows, HTTP stack, refresh tokens) are in `auth.md`.

## Layout

Below `$XDG_DATA_HOME/gdcli/` (default `~/.local/share/gdcli/`), directory mode 0700. Google commands create nothing; `auth login`, `auth setup` and the one-time migration create the directories.

```
default                         0600   "alice@example.com\n"
accounts/                       0700
  alice@example.com.json        0600   yup-oauth2 token file (same format as the old token.json)
  .login-XXXXXX                 0600   transient while a login runs
token.json                      legacy (0.3.0), read only while accounts/ holds no account
```

## Email

`auth::accounts::Email` is a validated newtype and the only way to build an account file name, so a path cannot leave `accounts/` by construction. The value is trimmed and ASCII-lowercased; the shape is `[a-z0-9._+-]+@[a-z0-9.-]+` with exactly one `@`, at most 128 bytes, no leading `.` and no `..`. Anything else (`/`, `\`, NUL, spaces, non-ASCII, empty) is rejected: clap prints `invalid value 'x' for '--account <EMAIL>': expected an email address such as name@example.com` (exit 2), and an address that Google reports but the shape check refuses is `unsupported email address`. The file is `<email>.json`. Temporary files start with `.` and never parse as an account; `list` also ignores directories, names that are not their own canonical form (`Alice@Example.com.json`) and other files. Gmail dot and plus aliases are not folded: the identity is the address Google reports for the Drive user.

## Choosing the account

1. `--account <email>` (`-a`), on the Google leaf commands only (every `meet`, `calendar` and `docs` subcommand; not on `auth *`, `update`, `help`): parsed and lowercased by clap.
2. The `default` pointer file, parsed through `Email`; a tampered pointer is an error (`` invalid default account in <path>: ...; run `gdcli auth set-default <email>` ``).
3. With no pointer, the single existing account.
4. Otherwise an error. There is no environment variable.

The account file must exist (`token_file`). Errors (stderr, exit 1, nothing on stdout; `<known>` is the sorted, comma separated list):

| Situation | Message |
|---|---|
| no account at all | `` no account is logged in: run `gdcli auth login` `` |
| `--account` names no known account | `unknown account <email>; known accounts: <known>` |
| the pointer names an account without a file | `` default account <email> has no token: run `gdcli auth login` or `gdcli auth set-default <email>`; known accounts: <known> `` |
| several accounts and no pointer | `` no default account; run `gdcli auth set-default <email>`; known accounts: <known> `` |
| token expired or revoked, no refresh possible | `` token retrieval failed: Invalid user input: no valid token for <email>: run `gdcli auth login` and choose that account `` (the legacy file: `` no valid existing token: run `gdcli auth login` ``) |
| the token file cannot be parsed | `` cannot load <file>: run `gdcli auth login` `` |
| the client secret is missing | `` cannot read client secret <path>: run `gdcli auth setup` `` (reported before any account problem) |

## Commands

- `gdcli auth login`: creates `accounts/` (0700), reads the client secret, runs the migration best effort under the 120 s deadline (`let _ = with_deadline(...)`, so a revoked legacy token or a hanging call never blocks the login that replaces it), creates `accounts/.login-XXXXXX` containing `[]`, runs the browser flow against that file (no deadline: it waits for the human), asks Drive which account signed in (120 s deadline), then renames the finished file to `accounts/<email>.json` and prints `account:`, `status:` (`added`, or `refreshed` when the file existed) and `default:`. The first account (no account existed before the rename) becomes the default and overwrites any stale pointer; later accounts leave the default alone; a refresh keeps it. No stored token is deleted: a failed, timed-out or aborted login drops the temporary file and every account stays as it was.
- `gdcli auth list`: header `account<TAB>default`, one row per account sorted by email, `default` is `true` or `false`. The pointer is read leniently, so a stale pointer, or several accounts without a pointer, only means that no row is marked; `auth list` never fails on an ambiguous default. No `--json`. Runs the migration first.
- `gdcli auth set-default <email>`: the account must exist (else `unknown account`); the pointer is replaced atomically (temporary file in the data directory, `persist`, mode 0600); prints `account:` and `default: true`. Runs the migration first. The top-level `gdcli set-default` is deliberately not built (no aliases).
- There is no `auth remove` or logout (not requested): delete `accounts/<email>.json` by hand; if it was the default, the pointer is stale and the single-account rule or the `set-default` hint repairs it.
- `gdcli auth setup` is the 0.3.0 wizard (`auth.md`) followed by `auth login`.

## Migration of the 0.3.0 token

Lazy and mandatory for every Google command, `auth list` and `auth set-default` (a failure is an error with the original cause); best effort for `auth login`. `migrate` does one `stat` when there is no `token.json` (the fast path); with a stray `token.json` it also reads `accounts/` and returns when an account exists. Otherwise it builds an authenticator on the legacy file (a refresh may rewrite it first, as before), asks `GET drive/v3/about?fields=user(emailAddress)` which account it is (`drive.readonly` covers it: no new scope, no new login), then `adopt`: create `accounts/` (0700), write the pointer (unconditionally: `accounts/` is empty here, so any pointer is stale), `rename(token.json, accounts/<email>.json)` (the file itself moves, nothing is copied or deleted before the new name exists) and force mode 0600.

- Two processes may migrate at once: both discover the same address; the loser's `rename` fails with `NotFound` and is accepted because the target exists. A refresh that re-creates `token.json` after the move leaves a stray legacy file that is ignored while accounts exist (and may be deleted by hand). Two first logins at the same time are harmless: both account files are written atomically and the pointer write is atomic, the last wins.
- A crash leaves at most a `.login-*` file (ignored by `list`, safe to delete) or an account without a pointer (the single-account rule or the `set-default` hint repairs it).
- A downgrade to 0.3.0 after the migration finds no `token.json` and asks for a login.

## The Drive dependency

The address comes from Drive `about.get`, so the Drive API must be enabled for the project (`docs/setup.md` lists it since 0.1.0; before 0.4.0 only `docs` needed it). Every discovery failure is reported as `cannot identify the account: <cause>`; only when the error chain holds a Google API error with HTTP status 403 or 404 the suffix ` (enable the Google Drive API for the project)` is appended (a network error or a 500 stays plain). A response without `user.emailAddress` is `cannot identify the account: google did not report an email address for this account`. A Workspace account with Drive switched off, or a project without the Drive API, can neither log in nor migrate. Rejected: a Calendar based fallback (`calendars.get` and `calendarList.get` do not accept `calendar.events.readonly`; an events based guess equals the address only by convention), an ID token or `userinfo` (a new scope would force every user to log in again).

## Atomicity and permissions

- Names: only `Email` reaches the file system.
- Directories 0700 (`DirBuilder`), token files 0600 (`NamedTempFile`, yup-oauth2, and `adopt` enforces it after the rename), `default` 0600; tests assert the modes.
- Accounts appear by `rename` of a complete file; the pointer is replaced by `rename`.
- yup-oauth2 rewrites a token file in place when it refreshes (existing behaviour, now limited to one account; a crash during a refresh can truncate that one file, and `gdcli auth login` repairs it).

## Consent screen facts

An external app in publishing status "Testing" admits at most 100 test users (each account must be listed) and expires authorizations and refresh tokens after 7 days; "Internal" admits only members of the Google Cloud organization (`org_internal` otherwise); a "Personal" project admits only the account that created it, so it cannot have a second account; at most 100 refresh tokens exist per Google account and client (the oldest is invalidated silently). Sources: https://developers.google.com/identity/protocols/oauth2 (refresh token expiration) and https://support.google.com/cloud/answer/15549945. Written for users in `docs/setup.md`.
