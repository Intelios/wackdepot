# AGENTS.md

WackDepot is a native macOS Git GUI — a GitHub-Desktop-class app rebuilt in Rust on [gpui](https://github.com/zeronsh/zui) (the zui fork), with a frosted-glass shell and playful motion. macOS on Apple Silicon only; don't "fix" Unix-only APIs for other platforms.

## Roles

- **Jack (owner): Designer.** UI design, mascot/brand mark, iconography, copy direction, product calls.
- **The agent (pi): Developer.** All code, tooling, technical decisions — and **all git operations on this repository**. The agent has standing permission to commit and pull autonomously, on its own volition, without asking. Commit early and often on `main`; push only once a remote exists. Never rewrite pushed history or touch the user's other repositories.

## Ground rules

- **Git goes through the system `git` CLI** (subprocess, parsed output). No libgit2/git2 — deliberate; subprocess inherits the user's credential helpers and avoids libgit2 edge cases. Always set `GIT_TERMINAL_PROMPT=0` so a GUI-launched git never blocks on a tty prompt.
- **Errors are user-facing sentences.** Fallible functions return `Result<T, String>` whose `Err` reads as a sentence shown verbatim in the UI. No `anyhow`/`thiserror`.
- **Theme tokens only.** Every colour comes from `Theme` in `crates/ui/src/theme.rs`; never a raw `rgb()`/`hsla()` literal outside it. The accent (lime `#c2ee4a` default) recolours interaction, never meaning: danger, warning, diff and success colours stay fixed in every theme.
- **Shell vs content.** Only shell surfaces (sidebar rail, titlebar, popovers) turn translucent over the window blur — content surfaces stay solid so text stays readable.
- **Motion is identity but cheap.** Animate transforms and opacity; honour reduced motion where gpui exposes it.
- **macOS, not web.** Hidden-inset titlebar with app-owned drag strip, ⌘⇧⌥⌃ key glyphs, menus named as macOS menus.

## Map

- **`crates/core`** (`wackdepot-core`): all git logic — subprocess runner, porcelain-v2 status parsing, unified-diff parsing + hunk surgery, `git apply` staging/discard, commit, log, branches, fetch/pull/push, stash, clone. Pure and testable; no gpui.
- **`crates/ui`** (`wackdepot-ui`): the gpui app — `theme.rs` (tokens), `glass.rs`/`frost.rs` (glass plates + backdrop-blur, ported from zeron), `input.rs` (TextInput), `icons.rs` (embedded SVGs + `icon()`), `motion.rs` (menu_in/dialog_in/fade_in/slide_in/pulse), `settings.rs` (`wackdepot.json`), `watcher.rs` (notify), `state.rs` (Tab/Dialog/PopoverKind/BusyOp/Notice), `shell.rs` (entity: state + refresh pipeline + `run_op`), `render.rs` (`impl Render`: titlebar/rail/content/layers), `changes.rs`, `diffview.rs`, `history.rs`, `pickers.rs`, `dialogs.rs`, `ops.rs`, `welcome.rs`, `popover.rs` (frosted anchored helper), `lib.rs` (`run()`).
- **`apps/wackdepot`**: thin binary (`main.rs`) + bundle metadata.
- **`scripts/`**: `bundle.sh` (release build → `dist/WackDepot.app`, ad-hoc signed), `make_icon.py` (placeholder icon PNG, no deps). `assets/icon.icns` is generated and gitignored.

## Conventions

- **gpui** and `gpui_platform` come from `github.com/zeronsh/zui`, pinned by rev in `Cargo.toml` — needed for `paint_backdrop_blur` and the macOS 26+ window-blur fix. Don't bump the rev casually; check zeron's bump notes first.
- **Rust:** serde-defaulted fields on anything persisted to `wackdepot.json` (`~/Library/Application Support/WackDepot/`). Tests live in `#[cfg(test)]` modules beside the code; core tests run real `git` in `tempfile` dirs.
- **Persistence:** everything under the app support dir; never write into a user's repository except real git operations they asked for.

## Commands

| Command | When |
|---|---|
| `cargo run -p wackdepot` | Launch the app (dev). |
| `cargo test --workspace` | All tests (core runs real git in tempdirs). |
| `cargo clippy --workspace -- -D warnings` | Lint gate. |
| `cargo fmt --all` | Format. |
| `sh scripts/bundle.sh` | Build `dist/WackDepot.app` (release + icon + ad-hoc sign). |

## Ported code

Patterns ported from zeron (`crates/ui`: glass.rs, frost.rs, window setup) and gpui's `input.rs` example — Apache-2.0/MIT, attributed in `THIRD_PARTY_NOTICES.md`.
