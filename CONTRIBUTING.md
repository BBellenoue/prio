# Contributing

Thanks for taking the time. Bug reports and pull requests are welcome.

## Before opening a pull request

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

The same steps run in CI on every pull request, on Windows and on macOS,
alongside a
security workflow (Semgrep, Trivy, cargo-deny, Gitleaks). A new dependency has
to pass `cargo deny check`: no unpatched RustSec advisory, a licence in the
allow list of `deny.toml`, and crates.io as its only source.

A few things that are easy to miss:

- **Everything system-specific lives in `src/platform/`.** One file per system
  behind the same functions, no `cfg` scattered through the UI. A change that
  touches the shortcuts, the tray icon or the data folder has to land in both
  `win.rs` and `mac.rs`, and CI builds both.
- **No assets.** Fonts come from the system, the icons are drawn in code. Keep
  it that way.
- **No personal data.** Task files, backups and debug logs live in the user's
  data folder and never in the repository. Tests use made-up names and
  `example.com`.
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

Open an [issue](https://github.com/BBellenoue/prio/issues). Include the system
and its version, the commit you built from, the steps that reproduce it and,
when the resident is involved, the relevant lines of `debug.log` written next
to `tasks.json` with `PRIO_DEBUG=1`.

For anything security-related, follow [SECURITY.md](SECURITY.md) instead of
opening a public issue.

## Licence

Contributions are accepted under the [MIT licence](LICENSE) that covers the
project.
