# Contributing

Thanks for taking the time. Bug reports and pull requests are welcome.

## Before opening a pull request

```powershell
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

The same steps run in CI on every pull request, on Windows.

A few things that are easy to miss:

- **Windows only, on purpose.** The resident relies on `RegisterHotKey`,
  `GetLocalTime` and the notification area. Portability is not a goal.
- **No assets.** Fonts come from `C:\Windows\Fonts`, the tray icon is drawn in
  code. Keep it that way.
- **No personal data.** Task files, backups and debug logs live in `%APPDATA%`
  and never in the repository. Tests use made-up names and `example.com`.
- **Stay on the standard library** where it does the job. A new crate needs a
  reason a few lines of code cannot give.
- **Plain hyphens.** No em dash or en dash in prose, comments, strings or
  commit messages; use the punctuation the dash stood for.

## Commits

One logical change per commit. Subject in the imperative, no trailing period,
no prefix or ticket number:

```
Sink tasks that wait on someone to the bottom of the list
```

Use the body to explain *why*: the constraint, the failure it avoids, what you
verified. The code says what changed; the message is where the reasoning is
kept, so it does not need to be repeated in comments.

## Reporting a bug

Open an [issue](https://github.com/BBellenoue/prio/issues). Include the Windows
version, the commit you built from, the steps that reproduce it and, when the
resident is involved, the relevant lines of `%APPDATA%\prio\debug.log` written
with `PRIO_DEBUG=1`.

For anything security-related, follow [SECURITY.md](SECURITY.md) instead of
opening a public issue.

## Licence

Contributions are accepted under the [MIT licence](LICENSE) that covers the
project.
