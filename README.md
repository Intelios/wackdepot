# WackDepot

A native macOS Git GUI — the spirit of GitHub Desktop, rebuilt in Rust on
[gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui) (via the
[zui fork](https://github.com/zeronsh/zui)) with a frosted-glass shell.

Review changes, stage files and hunks, commit, browse history, manage branches,
and fetch/pull/push — fast, native, and a little playful.

GitHub integration adds a native Pull Requests workspace for creation, diffs,
inline reviews, conversations, checks, and guarded merging. See
[GitHub setup](docs/GITHUB_SETUP.md) for OAuth registration and integration tests.

## Status

Early development. Requires macOS on Apple Silicon and the system `git`.

```sh
cargo run -p wackdepot
```

Part of the Wack family, alongside [WackCode](../WackCode).
