//! Text fields: single-line (scrolls sideways) and multi-line (wraps, grows
//! from `min_rows` to `max_rows`, then scrolls). Grew out of gpui's
//! `examples/input.rs` (Apache-2.0): IME, selection, clipboard; plus word
//! and line movement, undo/redo, and theme colours read at paint time.

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, KeyBinding, KeyContext, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, point,
    prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::ActiveTheme;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocStart,
        DocEnd,
        SelectAll,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Newline,
        Submit,
        ShowCharacterPalette,
    ]
);

/// Bind the field keys (scoped to focused fields).
pub fn bind_keys(cx: &mut App) {
    let c = Some("TextInput");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("alt-backspace", DeleteWordLeft, c),
        KeyBinding::new("alt-delete", DeleteWordRight, c),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("alt-left", WordLeft, c),
        KeyBinding::new("alt-right", WordRight, c),
        KeyBinding::new("alt-shift-left", SelectWordLeft, c),
        KeyBinding::new("alt-shift-right", SelectWordRight, c),
        KeyBinding::new("cmd-left", LineStart, c),
        KeyBinding::new("cmd-right", LineEnd, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("cmd-shift-left", SelectLineStart, c),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
        KeyBinding::new("cmd-up", DocStart, c),
        KeyBinding::new("cmd-down", DocEnd, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("cmd-c", Copy, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("enter", Submit, c),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, c),
        // Multi-line fields take Enter as a newline (bound after Submit so
        // it wins inside a TextArea).
        KeyBinding::new("enter", Newline, Some("TextArea")),
        KeyBinding::new("shift-enter", Newline, Some("TextArea")),
    ]);
}

/// What a field tells its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEvent {
    Changed,
    /// Enter in a single-line field.
    Submit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Single,
    Multi { min_rows: usize, max_rows: usize },
}

#[derive(Clone)]
struct Snapshot {
    text: String,
    sel: Range<usize>,
}

pub struct TextInput {
    focus: FocusHandle,
    content: String,
    placeholder: SharedString,
    mode: Mode,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<Instant>,
    /// Remembered caret x for up/down.
    goal_x: Option<Pixels>,
    selecting: bool,
    /// Horizontal scroll (single) or vertical scroll (multi), in px.
    scroll: Pixels,
    layout: Option<Laid>,
}

/// The last painted layout, for hit-testing and IME geometry.
struct Laid {
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    lines: Lines,
}

enum Lines {
    Single(Box<ShapedLine>),
    /// One wrapped line per paragraph, with its byte start and y offset.
    Multi(Vec<(usize, Pixels, WrappedLine)>),
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            content: String::new(),
            placeholder: placeholder.into(),
            mode: Mode::Single,
            selected: 0..0,
            reversed: false,
            marked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            goal_x: None,
            selecting: false,
            scroll: px(0.),
            layout: None,
        }
    }

    /// A wrapping field that grows from `min_rows` to `max_rows` lines.
    pub fn multiline(mut self, min_rows: usize, max_rows: usize) -> Self {
        self.mode = Mode::Multi {
            min_rows: min_rows.max(1),
            max_rows: max_rows.max(min_rows.max(1)),
        };
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn set_content(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        let text = if self.mode == Mode::Single {
            text.replace('\n', " ")
        } else {
            text
        };
        self.content = text;
        self.selected = self.content.len()..self.content.len();
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
        self.scroll = px(0.);
    }

    pub fn set_placeholder(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = text.into();
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    // ── Editing core ───────────────────────────────────────────────────

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        self.reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify();
    }

    /// Snapshot for undo. Typing bursts (within 500ms) share one step.
    fn checkpoint(&mut self, coalesce: bool) {
        let now = Instant::now();
        let recent = self
            .last_edit
            .is_some_and(|t| now - t < Duration::from_millis(500));
        if !(coalesce && recent) {
            self.undo.push(Snapshot {
                text: self.content.clone(),
                sel: self.selected.clone(),
            });
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some(now);
    }

    fn replace(&mut self, range: Range<usize>, text: &str, coalesce: bool, cx: &mut Context<Self>) {
        self.checkpoint(coalesce);
        let text = if self.mode == Mode::Single {
            text.replace(['\n', '\r'], " ")
        } else {
            text.replace('\r', "")
        };
        self.content.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        self.goal_x = None;
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn prev_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < offset).then_some(i))
            .unwrap_or(0)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > offset).then_some(i))
            .unwrap_or(self.content.len())
    }

    fn prev_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .rev()
            .find_map(|(i, _)| (i < offset).then_some(i))
            .unwrap_or(0)
    }

    fn next_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .find_map(|(i, w)| (i + w.len() > offset).then_some(i + w.len()))
            .unwrap_or(self.content.len())
    }

    fn line_start(&self, offset: usize) -> usize {
        self.content[..offset]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0)
    }

    fn line_end(&self, offset: usize) -> usize {
        self.content[offset..]
            .find('\n')
            .map(|i| offset + i)
            .unwrap_or(self.content.len())
    }

    // ── Actions ────────────────────────────────────────────────────────

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let prev = self.prev_grapheme(self.cursor());
            if prev == self.cursor() {
                window.play_system_bell();
                return;
            }
            self.selected = prev..self.cursor();
        }
        self.replace(self.selected.clone(), "", true, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let next = self.next_grapheme(self.cursor());
            if next == self.cursor() {
                window.play_system_bell();
                return;
            }
            self.selected = self.cursor()..next;
        }
        self.replace(self.selected.clone(), "", true, cx);
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.selected = self.prev_word(self.cursor())..self.cursor();
        }
        self.replace(self.selected.clone(), "", false, cx);
    }

    fn delete_word_right(&mut self, _: &DeleteWordRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.selected = self.cursor()..self.next_word(self.cursor());
        }
        self.replace(self.selected.clone(), "", false, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected.is_empty() {
            self.selected = self.line_start(self.cursor())..self.cursor();
        }
        self.replace(self.selected.clone(), "", false, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = if self.selected.is_empty() {
            self.prev_grapheme(self.cursor())
        } else {
            self.selected.start
        };
        self.move_to(to, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = if self.selected.is_empty() {
            self.next_grapheme(self.cursor())
        } else {
            self.selected.end
        };
        self.move_to(to, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.prev_grapheme(self.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_grapheme(self.cursor()), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.prev_word(self.cursor()), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_word(self.cursor()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.prev_word(self.cursor()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_word(self.cursor()), cx);
    }

    fn line_start_action(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_start(self.cursor()), cx);
    }

    fn line_end_action(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_end(self.cursor()), cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_start(self.cursor()), cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_end(self.cursor()), cx);
    }

    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn vertical(&mut self, rows: i32, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Single {
            // Up/down in a one-line field jump to the ends, like macOS.
            let to = if rows < 0 { 0 } else { self.content.len() };
            if extend {
                self.select_to(to, cx)
            } else {
                self.move_to(to, cx)
            }
            return;
        }
        let Some((pos, lh)) = self.position_for(self.cursor()) else {
            return;
        };
        let x = *self.goal_x.get_or_insert(pos.x);
        let target = point(x, pos.y + lh * rows as f32 + lh / 2.);
        let to =
            self.index_for_local(target)
                .unwrap_or(if rows < 0 { 0 } else { self.content.len() });
        let goal = self.goal_x;
        if extend {
            self.select_to(to, cx)
        } else {
            self.move_to(to, cx)
        }
        self.goal_x = goal;
        let _ = window;
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, false, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, false, window, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, true, window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, true, window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0..self.content.len();
        self.reversed = false;
        cx.notify();
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(
                self.marked.clone().unwrap_or(self.selected.clone()),
                &text,
                false,
                cx,
            );
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected.clone()].to_string(),
            ));
            self.replace(self.selected.clone(), "", false, cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snapshot {
                text: self.content.clone(),
                sel: self.selected.clone(),
            });
            self.content = s.text;
            self.selected = s.sel;
            self.last_edit = None;
            cx.emit(TextInputEvent::Changed);
            cx.notify();
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snapshot {
                text: self.content.clone(),
                sel: self.selected.clone(),
            });
            self.content = s.text;
            self.selected = s.sel;
            self.last_edit = None;
            cx.emit(TextInputEvent::Changed);
            cx.notify();
        }
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.replace(self.selected.clone(), "\n", false, cx);
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Submit);
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ── Mouse ──────────────────────────────────────────────────────────

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.selecting = true;
        let at = self.index_for_point(event.position);
        if event.click_count >= 3 {
            self.selected = self.line_start(at)..self.line_end(at);
            cx.notify();
        } else if event.click_count == 2 {
            let start = self.prev_word(self.next_grapheme(at).min(self.content.len()));
            let end = self.next_word(start);
            self.selected = start..end.max(start);
            cx.notify();
        } else if event.modifiers.shift {
            self.select_to(at, cx);
        } else {
            self.move_to(at, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let at = self.index_for_point(event.position);
            self.select_to(at, cx);
        }
    }

    // ── Geometry ───────────────────────────────────────────────────────

    /// Caret position for `index`, relative to the text origin (scroll
    /// excluded), and the line height.
    fn position_for(&self, index: usize) -> Option<(Point<Pixels>, Pixels)> {
        let laid = self.layout.as_ref()?;
        match &laid.lines {
            Lines::Single(line) => Some((point(line.x_for_index(index), px(0.)), laid.line_height)),
            Lines::Multi(paras) => {
                let (start, y, line) = paras.iter().rev().find(|(s, _, _)| *s <= index)?;
                let p = line.position_for_index(index - start, laid.line_height)?;
                Some((point(p.x, *y + p.y), laid.line_height))
            }
        }
    }

    /// Text index for a point relative to the text origin.
    fn index_for_local(&self, p: Point<Pixels>) -> Option<usize> {
        let laid = self.layout.as_ref()?;
        match &laid.lines {
            Lines::Single(line) => Some(line.closest_index_for_x(p.x)),
            Lines::Multi(paras) => {
                if p.y < px(0.) {
                    return Some(0);
                }
                let (start, y, line) = paras
                    .iter()
                    .rev()
                    .find(|(_, y, _)| *y <= p.y)
                    .or_else(|| paras.first())?;
                let local = point(p.x, p.y - *y);
                let i = match line.closest_index_for_position(local, laid.line_height) {
                    Ok(i) | Err(i) => i,
                };
                Some(start + i.min(line.len()))
            }
        }
    }

    fn index_for_point(&self, window_point: Point<Pixels>) -> usize {
        let Some(laid) = self.layout.as_ref() else {
            return 0;
        };
        let origin = laid.bounds.origin;
        let local = match laid.lines {
            Lines::Single(_) => point(window_point.x - origin.x + self.scroll, px(0.)),
            Lines::Multi(_) => point(
                window_point.x - origin.x,
                window_point.y - origin.y + self.scroll,
            ),
        };
        self.index_for_local(local).unwrap_or(self.content.len())
    }

    // ── UTF-16 bridging for IME ────────────────────────────────────────

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.replace(range, text, true, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.content.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selected = new_selected
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.start)
            .unwrap_or_else(|| range.start + text.len()..range.start + text.len());
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let (start, lh) = self.position_for(range.start)?;
        let (end, _) = self.position_for(range.end)?;
        let scroll = match self.layout.as_ref()?.lines {
            Lines::Single(_) => point(-self.scroll, px(0.)),
            Lines::Multi(_) => point(px(0.), -self.scroll),
        };
        Some(Bounds::from_corners(
            bounds.origin + start + scroll,
            bounds.origin + point(end.x, end.y + lh) + scroll,
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_point(p)))
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut ctx = KeyContext::new_with_defaults();
        ctx.add("TextInput");
        if matches!(self.mode, Mode::Multi { .. }) {
            ctx.add("TextArea");
        }
        div()
            .id(ElementId::Name("text-input".into()))
            .key_context(ctx)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start_action))
            .on_action(cx.listener(Self::line_end_action))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .overflow_hidden()
            .child(TextElement { input: cx.entity() })
    }
}

/// Paints the text, selection, caret and placeholder.
struct TextElement {
    input: Entity<TextInput>,
}

struct Prepaint {
    lines: Option<Lines>,
    quads: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
    line_height: Pixels,
    offset: Point<Pixels>,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl TextElement {
    fn runs(
        input: &TextInput,
        text_len: usize,
        color: gpui::Hsla,
        font: gpui::Font,
    ) -> Vec<TextRun> {
        let run = TextRun {
            len: text_len,
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        match &input.marked {
            Some(m) if !input.content.is_empty() => [
                TextRun {
                    len: m.start,
                    ..run.clone()
                },
                TextRun {
                    len: m.end - m.start,
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text_len - m.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect(),
            _ => vec![run],
        }
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let input = self.input.read(cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let line_height = window.line_height();
        match input.mode {
            Mode::Single => {
                style.size.height = line_height.into();
                (window.request_layout(style, [], cx), ())
            }
            Mode::Multi { min_rows, max_rows } => {
                let content = input.content.clone();
                let placeholder = input.placeholder.clone();
                let text_style = window.text_style();
                let layout =
                    window.request_measured_layout(style, move |known, available, window, _| {
                        let width = known.width.or(match available.width {
                            gpui::AvailableSpace::Definite(w) => Some(w),
                            _ => None,
                        });
                        let text: SharedString = if content.is_empty() {
                            placeholder.clone()
                        } else {
                            content.clone().into()
                        };
                        let font_size = text_style.font_size.to_pixels(window.rem_size());
                        let run = TextRun {
                            len: text.len(),
                            font: text_style.font(),
                            color: text_style.color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let rows = window
                            .text_system()
                            .shape_text(text, font_size, &[run], width, None)
                            .map(|lines| {
                                lines
                                    .iter()
                                    .map(|l| l.wrap_boundaries().len() + 1)
                                    .sum::<usize>()
                            })
                            .unwrap_or(1);
                        let rows = rows.clamp(min_rows, max_rows);
                        size(width.unwrap_or(px(100.)), line_height * rows as f32)
                    });
                (layout, ())
            }
        }
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let theme = cx.theme().clone();
        let input = self.input.read(cx);
        let style = window.text_style();
        let line_height = window.line_height();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let empty = input.content.is_empty();
        let (text, color): (SharedString, _) = if empty {
            (input.placeholder.clone(), theme.placeholder)
        } else {
            (input.content.clone().into(), style.color)
        };
        let selection_color = theme.accent.opacity(0.28);
        let caret_color = theme.accent;
        let sel = input.selected.clone();
        let cursor = input.cursor();
        let mut quads = Vec::new();
        let mut caret = None;
        let offset;

        let lines = match input.mode {
            Mode::Single => {
                let runs = TextElement::runs(input, text.len(), color, style.font());
                let line = window
                    .text_system()
                    .shape_line(text, font_size, &runs, None);
                let cx_pos = if empty {
                    px(0.)
                } else {
                    line.x_for_index(cursor)
                };
                // Keep the caret in view.
                let mut scroll = input.scroll;
                let width = bounds.size.width - px(2.);
                if cx_pos - scroll > width {
                    scroll = cx_pos - width;
                } else if cx_pos < scroll {
                    scroll = cx_pos;
                }
                scroll = scroll.max(px(0.)).min((line.width() - width).max(px(0.)));
                offset = point(-scroll, px(0.));
                if !empty && !sel.is_empty() {
                    quads.push(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + line.x_for_index(sel.start) - scroll,
                                bounds.top(),
                            ),
                            point(
                                bounds.left() + line.x_for_index(sel.end) - scroll,
                                bounds.bottom(),
                            ),
                        ),
                        selection_color,
                    ));
                }
                caret = Some(fill(
                    Bounds::new(
                        point(bounds.left() + cx_pos - scroll, bounds.top()),
                        size(px(2.), line_height),
                    ),
                    caret_color,
                ));
                Lines::Single(Box::new(line))
            }
            Mode::Multi { .. } => {
                let mut paras = Vec::new();
                let mut y = px(0.);
                let mut start = 0usize;
                let pieces: Vec<&str> = text.split('\n').collect();
                for piece in pieces {
                    let ptext: SharedString = piece.to_string().into();
                    let mut runs = vec![TextRun {
                        len: ptext.len(),
                        font: style.font(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }];
                    if ptext.is_empty() {
                        runs.clear();
                    }
                    let shaped = window
                        .text_system()
                        .shape_text(ptext, font_size, &runs, Some(bounds.size.width), None)
                        .ok()
                        .and_then(|mut v| v.pop());
                    if let Some(line) = shaped {
                        let h = line.size(line_height).height.max(line_height);
                        paras.push((start, y, line));
                        y += h;
                    }
                    start += piece.len() + 1;
                }
                // Selection rectangles per visual row, and the caret.
                let pos = |i: usize| -> Option<Point<Pixels>> {
                    let (s, py, line) = paras.iter().rev().find(|(s, _, _)| *s <= i)?;
                    let p = line.position_for_index(i - s, line_height)?;
                    Some(point(p.x, *py + p.y))
                };
                let total = y;
                let mut scroll = input.scroll;
                if !empty && let Some(c) = pos(cursor) {
                    if c.y + line_height - scroll > bounds.size.height {
                        scroll = c.y + line_height - bounds.size.height;
                    } else if c.y < scroll {
                        scroll = c.y;
                    }
                }
                scroll = scroll
                    .max(px(0.))
                    .min((total - bounds.size.height).max(px(0.)));
                offset = point(px(0.), -scroll);
                if !empty
                    && !sel.is_empty()
                    && let (Some(a), Some(b)) = (pos(sel.start), pos(sel.end))
                {
                    let mut row_y = a.y;
                    while row_y <= b.y {
                        let x0 = if row_y == a.y { a.x } else { px(0.) };
                        let x1 = if row_y == b.y { b.x } else { bounds.size.width };
                        quads.push(fill(
                            Bounds::from_corners(
                                point(bounds.left() + x0, bounds.top() + row_y - scroll),
                                point(
                                    bounds.left() + x1.max(x0 + px(4.)),
                                    bounds.top() + row_y + line_height - scroll,
                                ),
                            ),
                            selection_color,
                        ));
                        row_y += line_height;
                    }
                }
                let c = if empty {
                    Some(point(px(0.), px(0.)))
                } else {
                    pos(cursor)
                };
                if let Some(c) = c {
                    caret = Some(fill(
                        Bounds::new(
                            point(bounds.left() + c.x, bounds.top() + c.y - scroll),
                            size(px(2.), line_height),
                        ),
                        caret_color,
                    ));
                }
                Lines::Multi(paras)
            }
        };
        Prepaint {
            lines: Some(lines),
            quads,
            caret,
            line_height,
            offset,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        pre: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for q in pre.quads.drain(..) {
                window.paint_quad(q);
            }
            let origin = bounds.origin + pre.offset;
            match pre.lines.as_ref() {
                Some(Lines::Single(line)) => {
                    let _ = line.paint(
                        origin,
                        pre.line_height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                Some(Lines::Multi(paras)) => {
                    for (_, y, line) in paras {
                        let _ = line.paint(
                            origin + point(px(0.), *y),
                            pre.line_height,
                            gpui::TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                }
                None => {}
            }
            if focus.is_focused(window)
                && let Some(c) = pre.caret.take()
            {
                window.paint_quad(c);
            }
        });
        let lines = pre.lines.take();
        let scroll = match pre.offset {
            p if p.x != px(0.) => -p.x,
            p => -p.y,
        };
        let line_height = pre.line_height;
        self.input.update(cx, |input, _| {
            input.scroll = scroll;
            input.layout = lines.map(|lines| Laid {
                bounds,
                line_height,
                lines,
            });
        });
    }
}
