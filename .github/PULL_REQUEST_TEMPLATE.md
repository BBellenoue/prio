## Why

<!-- The constraint or failure this change answers. The diff already says what. -->

## Checks

- [ ] `cargo fmt --check && cargo clippy -- -D warnings && cargo test` pass
- [ ] A change under `src/platform/` lands in `win.rs`, `mac.rs` and `linux.rs`
- [ ] No personal data, no em dash or en dash, no new crate without a reason
