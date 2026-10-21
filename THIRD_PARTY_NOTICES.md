# Third-party notices

WackDepot builds on and ports patterns from:

- **gpui** — Zed Industries' UI framework, used via the
  [zui](https://github.com/zeronsh/zui) fork (Apache-2.0). The fork carries
  identical crate sources to zed-industries/zed's gpui plus the BackdropBlur
  primitive, per-edge EdgeFade, and the macOS 26+ `UnderWindowBackground`
  window-blur fix.
- **zeron** (https://github.com/zeronsh/zeron, MIT) — the single-layer
  backdrop-blur wrapper (`crates/ui/src/components/frost.rs`), the titlebar
  drag-strip pattern, and the titlebar geometry.
- **gpui's `input.rs` example** (Apache-2.0) — `crates/ui/src/components/
  text_input.rs` grew out of the example's IME/selection/clipboard
  implementation.
- **syntect** (MIT) and **two-face** / bat's syntax set (MIT/Apache-2.0) —
  syntax highlighting in `crates/syntax`.
- **similar** (Apache-2.0) — token diffing for word-level highlights in
  `crates/core/src/words.rs`.
- **github/gitignore** (CC0-1.0) — the `.gitignore` templates vendored in
  `crates/core/gitignore/`, indexed by `build.rs` for the create-repository
  and ignore pickers.
- **github/choosealicense.com** (CC0-1.0) — the license texts vendored in
  `crates/core/licenses/` (the set GitHub Desktop's license picker shows,
  pinned to its submodule revision), indexed by `build.rs`. The license
  texts themselves carry their own licenses; choosealicense.com's content
  license is vendored alongside as `crates/core/licenses/CHOOSEALICENSE.md`.

## Inter (bundled UI font)

`assets/fonts/Inter-*.ttf` — Inter 4.1 by The Inter Project Authors
(https://github.com/rsms/inter), licensed under the SIL Open Font License 1.1.
The full license text ships beside the fonts in `assets/fonts/Inter-OFL.txt`.

## WackCode theme engine and glass

`crates/ui/src/theme/` ports the token recipes and colour maths of WackCode's
`src/theme.ts`, and `crates/ui/src/native/glass.rs` ports its
`src-tauri/src/glass.rs` (same author).
