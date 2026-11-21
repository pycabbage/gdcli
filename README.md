Agent-oriented CLI for Google Workspace: Meet, Calendar and Docs.

```sh
curl -fsSL -o /tmp/gdcli.tar.gz https://github.com/pycabbage/gdcli/releases/latest/download/gdcli_linux_$(uname -m).tar.gz
mkdir -p ~/.local/bin && tar -xzf /tmp/gdcli.tar.gz -C ~/.local/bin gdcli
gdcli auth setup
```

Skills are available via `gdcli skills`, or can be added to your system with:

```sh
npx skills add pycabbage/gdcli
```
