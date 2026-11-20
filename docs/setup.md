# Google Cloud setup for gdcli

gdcli reads Google Calendar, Google Meet and Google Docs with your own Google account (or several). It needs an OAuth client of type "Desktop app" that you create once.

1. Open https://console.cloud.google.com/ and create or select a project.
2. Enable these APIs (APIs & Services > Library): Google Calendar API, Google Meet REST API, Google Drive API, Google Docs API. The Drive API is needed for every login (gdcli asks it which account signed in) and for `gdcli docs get`; the Docs API is needed for `gdcli docs get --tab` and for the tab list of `gdcli docs metadata`; all other commands work without it.
3. Configure the OAuth consent screen (APIs & Services > OAuth consent screen, or Google Auth Platform): choose "External" for a personal account or "Internal" for a Workspace organisation, then add every Google account you want to use as a test user while the app is in "Testing" status.
4. Add these scopes to the consent screen:
   - https://www.googleapis.com/auth/calendar.events.readonly
   - https://www.googleapis.com/auth/meetings.space.readonly
   - https://www.googleapis.com/auth/drive.readonly
5. Create the OAuth client (APIs & Services > Credentials > Create credentials > OAuth client ID) with application type "Desktop app", then download its JSON.
6. Place the downloaded JSON at `$XDG_CONFIG_HOME/gdcli/client_secret.json` (default `~/.config/gdcli/client_secret.json`, file mode 0600), or give its path to the prompt below and gdcli copies it there.

Consent screen status and accounts:

- "Testing": at most 100 test users, each account must be listed, and Google expires the refresh token of a test authorization after 7 days, after which every command fails with "run `gdcli auth login`". Either publish the app (OAuth consent screen > Publish app, status "In production"; staying unverified is fine for personal use, Google only shows an "unverified app" warning during login) or run `gdcli auth login` again every 7 days.
- "Internal": only members of the Google Cloud organization can log in (other accounts fail with the `org_internal` error).
- "Personal" project: only the account that created the project can log in, so it cannot have a second account.
- Google keeps at most 100 refresh tokens per Google account and OAuth client; the oldest is invalidated silently.

After that gdcli prints an authorization URL. Open it in a browser on the same machine, choose the Google account, approve access, and the login finishes automatically through a local loopback redirect. gdcli then prints `account:`, `status:` (`added`, or `refreshed` for an account that was already known) and `default:`. The token of every account is stored at `$XDG_DATA_HOME/gdcli/accounts/<email>.json` (default `~/.local/share/gdcli/accounts/<email>.json`) and the first account becomes the default (`$XDG_DATA_HOME/gdcli/default`). Run `gdcli auth login` again and choose another account to add it, `gdcli auth list` to see the accounts, `gdcli auth set-default <email>` to change the default, and `--account <email>` (`-a`) on a command to use another account once.

Upgrading from 0.3.0: `gdcli setup` and `gdcli login` are now `gdcli auth setup` and `gdcli auth login` (there are no aliases). The first command after upgrading asks the Drive API which account the old `token.json` belongs to and moves it to `accounts/<email>.json`, so the Drive API must be enabled for the project; no new login is needed. If that command fails with `cannot identify the account: google api error 403: ... (enable the Google Drive API for the project)`, enable the Google Drive API (APIs & Services > Library) and run it again.

If a command fails with `403 ... Google Docs API has not been used in project ... before or it is disabled`, enable the Google Docs API for the project (APIs & Services > Library) and run the command again; no new login is needed because the scopes do not change.
