# AGENTS.md

WackDepot is a native macOS Git GUI — a GitHub-Desktop-class app rebuilt in Rust on [gpui](https://github.com/zeronsh/zui) (the zui fork), with a frosted-glass shell and playful motion. macOS on Apple Silicon only; don't "fix" Unix-only APIs for other platforms.

## Roles

- **Jack (owner): Designer.** UI design, mascot/brand mark, iconography, copy direction, product calls.
- **The agent (pi): Developer.** All code, tooling, technical decisions — and **all git operations on this repository**. The agent has standing permission to commit and pull autonomously, on its own volition, without asking. Commit early and often on `main`; push only once a remote exists. Never rewrite pushed history or touch the user's other repositories.

## Ground rules

- **Git goes through the system `git` CLI** (subprocess, parsed output). No libgit2/git2 — deliberate; subprocess inherits the user's credential helpers and avoids libgit2 edge cases. Always set `GIT_TERMINAL_PROMPT=0` so a GUI-launched git never blocks on a tty prompt.
- **Errors are user-facing sentences.** Fallible functions return `Result<T, String>` whose `Err` reads as a sentence shown verbatim in the UI. No `anyhow`/`thiserror`.
- **Theme tokens only.** Every colour comes from `Theme` in `crates/ui/src/theme.rs`; never a raw `rgb()`/`hsla()` literal outside it. The accent (lime `#c2ee4a` default) recolours interaction, never meaning: danger, warning, diff and success colours stay fixed in every theme.
- **Shell vs content.** The whole window is native Liquid Glass; only the shell (titlebar, left column, gaps) is a tint over it. Content (diff card, composer, dialogs, inputs, settings blocks) stays solid so text stays readable. Popovers and toasts are frosted (in-app blur).
- **Motion is identity but cheap.** gpui has no transforms or springs: animate position offsets and opacity through `motion::Motion` (frames are requested only while something moves). `with_animation` is only for stateless 0–1 loops — it asserts on overshooting curves and replays on remount. Honour reduce motion.
- **macOS, not web.** Hidden-inset titlebar with app-owned drag strip, ⌘⇧⌥⌃ key glyphs, menus named as macOS menus.

## Map

- **`crates/core`** (`wackdepot-core`): all git logic — subprocess runner (with streaming progress), porcelain-v2 status (+ in-progress merge/rebase/cherry-pick/revert, line stats), unified-diff parsing + hunk and line-level patch building, `git apply` staging/discard, commit (amend, co-authors), revert/cherry-pick, log + graph lanes, branches/tags, fetch/pull/push/clone, stash (by sha), word-level diff, file versions, global identity. Pure and testable; no gpui.
- **`crates/syntax`** (`wackdepot-syntax`): syntect + bat's syntax set (two-face), scopes reduced to six code kinds, with size/time budgets.
- **`crates/ui`** (`wackdepot-ui`): the gpui app.
  - `app/` — `run()`, `Workspace` (root view: routing, backdrop, column resizer, actions), `actions.rs` (actions, keys, menus), `backdrop.rs` (Liquid Glass / blur / solid, opaque while unfocused), `overlays.rs` (dialogs, toasts), `prefs.rs` (settings global), `window.rs`.
  - `model/` — `RepoModel` (snapshot, keyed selection, diff loading, serial op lane, epoch-guarded refresh, progress) and `ops.rs` (every git action).
  - `views/` — `titlebar.rs` (repo/branch pickers, sync pill, menus), `sidebar/` (Changes list, composer, History + graph), `diff/` (the diff card), `settings.rs`, `welcome.rs`, `dialogs.rs`.
  - `components/` — pills/buttons, marks, pill track, menus/popovers, text input (single/multi-line), seams, spinner, slider, mascot, misc.
  - `theme/` — WackCode's OKLCH token engine (golden-tested), presets, fixed semantic colours, metrics, bundled Inter. The only place colour literals live (`lint.rs` enforces it).
  - `motion/` — easings, springs, the per-view `Motion` store, presence.
  - `native/` — NSGlassEffectView, NSApp appearance, accessibility settings.
- **`apps/wackdepot`**: thin binary (`main.rs`) + bundle metadata.
- **`scripts/`**: `bundle.sh` (release build → `dist/WackDepot.app`, ad-hoc signed), `dev-bundle.sh` (debug build as "WackDepot Dev.app", launched with optional `KEY=VALUE` env), `make_icon.py` (placeholder icon PNG, no deps). `assets/icon.icns` is generated and gitignored.

## Conventions

- **gpui** and `gpui_platform` come from `github.com/zeronsh/zui`, pinned by rev in `Cargo.toml` — needed for `paint_backdrop_blur` and the macOS 26+ window-blur fix. Don't bump the rev casually; check zeron's bump notes first.
- **Rust:** serde-defaulted fields on anything persisted to `wackdepot.json` (`~/Library/Application Support/WackDepot/`). Tests live in `#[cfg(test)]` modules beside the code; core tests run real `git` in `tempfile` dirs.
- **Persistence:** everything under the app support dir; never write into a user's repository except real git operations they asked for.

## Commands

| Command | When |
|---|---|
| `cargo run -p wackdepot` | Launch the app (dev). `WACKDEPOT_OPEN=<path>` opens a repo, `WACKDEPOT_SLOWMO=8` slows motion. |
| `sh scripts/dev-bundle.sh` | Debug build as "WackDepot Dev.app" (real app identity for UI checks). |
| `cargo test --workspace` | All tests (core runs real git in tempdirs). |
| `cargo clippy --workspace -- -D warnings` | Lint gate. |
| `cargo fmt --all` | Format. |
| `sh scripts/bundle.sh` | Build `dist/WackDepot.app` (release + icon + ad-hoc sign). |

## Ported code

Patterns ported from zeron (frost, titlebar drag strip), WackCode (theme engine, glass, design language), and gpui's `input.rs` example — attributed in `THIRD_PARTY_NOTICES.md`, with Inter, syntect, two-face and similar.
