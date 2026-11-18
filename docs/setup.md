# Google Cloud setup for gdcli

gdcli reads Google Calendar, Google Meet and Google Docs with your own Google account. It needs an OAuth client of type "Desktop app" that you create once.

1. Open https://console.cloud.google.com/ and create or select a project.
2. Enable these APIs (APIs & Services > Library): Google Calendar API, Google Meet REST API, Google Drive API.
3. Configure the OAuth consent screen (APIs & Services > OAuth consent screen, or Google Auth Platform): choose "External" for a personal account or "Internal" for a Workspace organisation, then add your Google account as a test user while the app is in "Testing" status.
4. Add these scopes to the consent screen:
   - https://www.googleapis.com/auth/calendar.events.readonly
   - https://www.googleapis.com/auth/meetings.space.readonly
   - https://www.googleapis.com/auth/drive.readonly
5. Create the OAuth client (APIs & Services > Credentials > Create credentials > OAuth client ID) with application type "Desktop app", then download its JSON.
6. Place the downloaded JSON at `$XDG_CONFIG_HOME/gdcli/client_secret.json` (default `~/.config/gdcli/client_secret.json`, file mode 0600), or give its path to the prompt below and gdcli copies it there.

Consent screen status "Testing": Google expires refresh tokens issued to apps in "Testing" status after 7 days, after which every command fails with "run `gdcli login`". Either publish the app (OAuth consent screen > Publish app, status "In production"; staying unverified is fine for personal use, Google only shows an "unverified app" warning during login) or run `gdcli login` again every 7 days.

After that gdcli prints an authorization URL. Open it in a browser on the same machine, approve access, and the login finishes automatically through a local loopback redirect. The token is stored at `$XDG_DATA_HOME/gdcli/token.json` (default `~/.local/share/gdcli/token.json`).
