// Adapted from GPUI 0.2.2 examples/input.rs, Copyright Zed Industries.
// Licensed under Apache-2.0 (see the repository's LICENSE-APACHE).
// Native text services, UTF-16 IME ranges, grapheme movement and clipboard handling.
use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, size, App, Bounds, ClipboardItem, Context,
    CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun,
    UTF16Selection, UnderlineStyle, Window,
};
use unicode_segmentation::*;

use super::Colors;

/// `runs` with the bytes in `range` drawn in `color`, splitting the runs at its ends.
fn recolor(runs: Vec<TextRun>, range: Range<usize>, color: gpui::Hsla) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        let cuts = [
            start,
            range.start.clamp(start, end),
            range
                .end
                .clamp(start, end)
                .max(range.start.clamp(start, end)),
            end,
        ];
        for piece in cuts.windows(2).filter(|w| w[1] > w[0]) {
            let mut part = run.clone();
            part.len = piece[1] - piece[0];
            if piece[0] >= range.start && piece[1] <= range.end {
                part.color = color;
            }
            out.push(part);
        }
    }
    out
}

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Enter,
    ]
);

pub(super) struct TextInput {
    focus_handle: FocusHandle,
    pub(super) content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Vec<LayoutLine>,
    pub(super) multiline: bool,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// The palette's colors: fg (muted placeholder) on bg, edged by line, ringed by focus;
    /// selected text is accent-fg on accent.
    pub(super) colors: Colors,
}

impl TextInput {
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline {
                text
            } else {
                text.replace('\n', " ")
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify()
    }

    fn invalidate_layout(&mut self) {
        // Shaped glyph indices belong to the old UTF-8 text. Until the next paint,
        // neither pointer hit-testing nor IME geometry may reuse those indices.
        self.last_layout.clear();
        self.last_bounds = None;
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        let row = ((position.y - bounds.top()) / px(30.)).floor() as usize;
        let Some(line) = self
            .last_layout
            .get(row)
            .or_else(|| self.last_layout.last())
        else {
            return 0;
        };
        (line.start + line.shaped.closest_index_for_x(position.x - bounds.left()))
            .min(self.content.len())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    pub(super) fn new(value: &str, colors: Colors, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: value.to_owned().into(),
            placeholder: "Type here…".into(),
            selected_range: value.len()..value.len(),
            selection_reversed: false,
            marked_range: None,
            last_layout: Vec::new(),
            multiline: false,
            last_bounds: None,
            is_selecting: false,
            colors,
        }
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.selection_reversed = false;
        self.invalidate_layout();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| {
                let offset = |n| {
                    let mut units = 0;
                    new_text
                        .char_indices()
                        .find_map(|(i, ch)| {
                            if units >= n {
                                return Some(i);
                            }
                            units += ch.len_utf16();
                            None
                        })
                        .unwrap_or(new_text.len())
                };
                offset(r.start)..offset(r.end)
            })
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;
        self.invalidate_layout();

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (row, line) = self
            .last_layout
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start <= range.start)?;
        let y = bounds.top() + px(30.) * row as f32;
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.shaped.x_for_index(range.start - line.start),
                y,
            ),
            point(
                bounds.left()
                    + line.shaped.x_for_index(
                        range.end.min(line.start + line.shaped.text.len()) - line.start,
                    ),
                y + px(30.),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

struct LayoutLine {
    start: usize,
    shaped: ShapedLine,
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Vec<LayoutLine>,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let input = self.input.read(cx);
        let rows = if input.multiline {
            input.content.split('\n').count().max(3)
        } else {
            1
        };
        style.size.height = (window.line_height() * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let display = if input.content.is_empty() {
            input.placeholder.as_ref()
        } else {
            input.content.as_ref()
        };
        let color = if input.content.is_empty() {
            input.colors.muted
        } else {
            input.colors.fg
        };
        let mut lines = Vec::new();
        let mut selection = Vec::new();
        let mut cursor_quad = None;
        let mut start = 0;
        for (row, text) in display.split('\n').enumerate() {
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut runs = vec![run.clone()];
            if let Some(marked) = &input.marked_range {
                let from = marked.start.saturating_sub(start).min(text.len());
                let to = marked.end.saturating_sub(start).min(text.len());
                if from < to {
                    runs = vec![
                        TextRun {
                            len: from,
                            ..run.clone()
                        },
                        TextRun {
                            len: to - from,
                            underline: Some(UnderlineStyle {
                                color: Some(color),
                                thickness: px(1.),
                                wavy: false,
                            }),
                            ..run.clone()
                        },
                        TextRun {
                            len: text.len() - to,
                            ..run
                        },
                    ];
                    runs.retain(|run| run.len > 0);
                }
            }
            if !input.content.is_empty() {
                // Selected text is accent-fg on its accent fill, a pair the palettes test.
                let from = input
                    .selected_range
                    .start
                    .saturating_sub(start)
                    .min(text.len());
                let to = input
                    .selected_range
                    .end
                    .saturating_sub(start)
                    .min(text.len());
                runs = recolor(runs, from..to, input.colors.accent_fg);
            }
            let shaped =
                window
                    .text_system()
                    .shape_line(text.to_owned().into(), font_size, &runs, None);
            let y = bounds.top() + window.line_height() * row as f32;
            let selected = &input.selected_range;
            if selected.start < start + text.len() && selected.end > start {
                selection.push(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left()
                                + shaped.x_for_index(selected.start.saturating_sub(start)),
                            y,
                        ),
                        point(
                            bounds.left()
                                + shaped.x_for_index((selected.end - start).min(text.len())),
                            y + window.line_height(),
                        ),
                    ),
                    input.colors.accent,
                ));
            }
            let cursor = input.cursor_offset();
            if selected.is_empty() && cursor >= start && cursor <= start + text.len() {
                cursor_quad = Some(fill(
                    Bounds::new(
                        point(bounds.left() + shaped.x_for_index(cursor - start), y),
                        size(px(2.), window.line_height()),
                    ),
                    input.colors.focus,
                ));
            }
            lines.push(LayoutLine { start, shaped });
            start += text.len() + 1;
        }
        PrepaintState {
            lines,
            cursor: cursor_quad,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        for (row, line) in lines.iter().enumerate() {
            let origin = point(
                bounds.left(),
                bounds.top() + window.line_height() * row as f32,
            );
            let _ = line.shaped.paint(origin, window.line_height(), window, cx);
        }
        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.input.update(cx, |input, _cx| {
            input.last_layout = lines;
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .border_1()
            .rounded_md()
            .border_color(self.colors.line)
            .focus(|s| s.border_color(self.colors.focus))
            .line_height(px(30.))
            .text_size(px(16.))
            .child(
                div()
                    .min_h(px(if self.multiline { 98. } else { 38. }))
                    .w_full()
                    .p(px(4.))
                    .bg(self.colors.bg)
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Enter, Some("TextInput")),
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
    ]);
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn light() -> Colors {
        Colors::new(&crate::theme::Tokens::compiled(&ikigai_ui::palette::LIGHT))
    }

    #[test]
    fn recolor_paints_only_the_selected_bytes() {
        let run = |len| TextRun {
            len,
            font: gpui::font("Helvetica"),
            color: light().fg,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = recolor(vec![run(3), run(4)], 2..5, light().accent_fg);
        let shape: Vec<_> = runs
            .iter()
            .map(|r| (r.len, r.color == light().accent_fg))
            .collect();
        assert_eq!(shape, [(2, false), (1, true), (2, true), (2, false)]);
        let empty = recolor(vec![run(3)], 1..1, light().accent_fg);
        assert!(empty.iter().all(|r| r.color == light().fg));
        assert_eq!(empty.iter().map(|r| r.len).sum::<usize>(), 3);
    }

    #[gpui::test]
    fn ime_selection_replaces_the_direction_of_a_backward_selection(cx: &mut TestAppContext) {
        let (input, cx) =
            cx.add_window_view(|_, cx| TextInput::new("before 🦀 after", light(), cx));
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                // Select backward, then let the IME replace that selection with marked text.
                input.select_to(7, cx);
                assert!(input.selection_reversed);
                input.replace_and_mark_text_in_range(None, "e\u{301}🦀", Some(2..4), window, cx);
                assert_eq!(input.content.as_ref(), "before e\u{301}🦀");
                assert_eq!(input.marked_range, Some(7..14));
                assert_eq!(input.selected_range, 10..14);
                let selected = input.selected_text_range(false, window, cx).unwrap();
                assert_eq!(selected.range, 9..11);
                assert!(!selected.reversed, "the IME supplied a forward selection");
                assert_eq!(input.cursor_offset(), 14);
                // Shift-left must shrink the IME selection from its active end.
                input.select_left(&SelectLeft, window, cx);
                assert_eq!(input.selected_range, 10..10);
            });
        });
    }

    #[gpui::test]
    fn edits_do_not_hit_test_unicode_with_an_old_ascii_layout(cx: &mut TestAppContext) {
        let (input, cx) = cx.add_window_view(|_, cx| TextInput::new("abcd", light(), cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                let bounds = input.last_bounds.expect("input was painted");
                let x = input.last_layout[0].shaped.x_for_index(1);
                let position = point(bounds.left() + x, bounds.top() + px(1.));
                // Input and pointer events can arrive before the next paint. Byte 1 of
                // the previous ASCII layout is inside the newly inserted emoji.
                input.replace_text_in_range(Some(0..4), "🦀", window, cx);
                let offset = input.index_for_mouse_position(position);
                input.move_to(offset, cx);
                input.backspace(&Backspace, window, cx);
                assert_eq!(input.content.as_ref(), "🦀");
            });
        });
    }

    #[gpui::test]
    fn grapheme_deletion_and_large_multiline_paste_keep_valid_ranges(cx: &mut TestAppContext) {
        let (input, cx) =
            cx.add_window_view(|_, cx| TextInput::new("e\u{301}🦀\n日本語", light(), cx));
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.multiline = true;
                input.move_to(7, cx); // after the combining cluster and the emoji
                input.backspace(&Backspace, window, cx);
                assert_eq!(input.content.as_ref(), "e\u{301}\n日本語");
                assert_eq!(input.selected_range, 3..3);
                input.backspace(&Backspace, window, cx);
                assert_eq!(input.content.as_ref(), "\n日本語");
                assert_eq!(input.selected_range, 0..0);
                input.delete(&Delete, window, cx);
                assert_eq!(input.content.as_ref(), "日本語");
                input.select_all(&SelectAll, window, cx);
                let pasted = "e\u{301}🦀\n日本語\n".repeat(10000);
                cx.write_to_clipboard(ClipboardItem::new_string(pasted.clone()));
                input.paste(&Paste, window, cx);
                assert_eq!(input.content.as_ref(), pasted);
                assert_eq!(input.selected_range, pasted.len()..pasted.len());
                assert!(!input.selection_reversed);
            });
        });
    }
}
