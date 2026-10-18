# Third-party notices

WackDepot builds on and ports patterns from:

- **gpui** — Zed Industries' UI framework, used via the
  [zui](https://github.com/zeronsh/zui) fork (Apache-2.0). The fork carries
  identical crate sources to zed-industries/zed's gpui plus the BackdropBlur
  primitive, per-edge EdgeFade, and the macOS 26+ `UnderWindowBackground`
  window-blur fix.
- **zeron** (https://github.com/zeronsh/zeron, MIT) — the frosted-glass
  approach (window `Blurred` background, single-layer backdrop-blur wrapper,
  glass plate gradients/rims/shadows, titlebar drag-strip pattern) is ported
  from `crates/ui` (`glass.rs`, `frost.rs`, `shell.rs`).
- **gpui's `input.rs` example** (Apache-2.0) — the `TextInput` entity/
  `TextElement` pair in `crates/ui/src/input.rs` is a port of the example's
  IME/selection/clipboard implementation.

## Inter (bundled UI font)

`assets/fonts/Inter-*.ttf` — Inter 4.1 by The Inter Project Authors
(https://github.com/rsms/inter), licensed under the SIL Open Font License 1.1.
The full license text ships beside the fonts in `assets/fonts/Inter-OFL.txt`.

## WackCode theme engine and glass

`crates/ui/src/theme/` ports the token recipes and colour maths of WackCode's
`src/theme.ts`, and `crates/ui/src/native/glass.rs` ports its
`src-tauri/src/glass.rs` (same author).
