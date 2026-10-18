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
