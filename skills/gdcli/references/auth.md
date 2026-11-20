# gdcli auth

Accounts are Google accounts identified by email; each has its own token, and the first account that logs in is the default. Every Google command takes `-a, --account <EMAIL>` (case-insensitive) and otherwise uses the default. A new account must be admitted by the OAuth consent screen of the project: a test user while the app is in Testing (and Google then expires the token after 7 days), a member of the organization for an Internal app, only the creator for a Personal project. Tokens live in `$XDG_DATA_HOME/gdcli/accounts/<email>.json` and the default in `$XDG_DATA_HOME/gdcli/default`; a `.login-*` file in `accounts/` left by an interrupted login is safe to delete. There is no logout: delete the account file.

## gdcli auth setup

`gdcli auth setup`

For a human. Prints the Google Cloud instructions and the destination of `client_secret.json`, asks for the path of the downloaded OAuth client JSON (empty if it is already in place; quotes and a leading `~/` are accepted), installs it, then logs in like `auth login`.

## gdcli auth login

`gdcli auth login`

For a human. Prints an authorization URL: open it in a browser on the same machine, choose the Google account and approve; the command then finishes by itself. A new account is added; a known one is refreshed. The default changes only for the first account. No stored token is touched before the login succeeds.

```
account: bob@example.org
status: added
default: false
```

`status` is `added` or `refreshed`.

## gdcli auth list

`gdcli auth list`

Columns `account`, `default` (`true` or `false`), one row per account sorted by email, the header alone when there is none; no `--json`. No row is marked when several accounts exist and no default was chosen.

## gdcli auth set-default

`gdcli auth set-default <EMAIL>`

Makes a logged-in account the default; prints `account: <email>` and `default: true`.

Errors (all commands that choose an account):

- ``no account is logged in: run `gdcli auth login` ``
- `unknown account <email>; known accounts: <emails>`
- ``no default account; run `gdcli auth set-default <email>`; known accounts: <emails>``
- ``default account <email> has no token: run `gdcli auth login` or `gdcli auth set-default <email>`; known accounts: <emails>``
- ``no valid token for <email>: run `gdcli auth login` and choose that account`` (expired or revoked; it follows `token retrieval failed: Invalid user input: `)
- ``cannot read client secret <path>: run `gdcli auth setup` ``
- `cannot identify the account: <cause>` (login asks Google Drive which account signed in; Google answering 403 or 404 adds ` (enable the Google Drive API for the project)`)
