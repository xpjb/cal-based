use std::ops::Range;

use based::widget::sanscale::raw::{Caret, Layout, Motion, WordBoundaries};
use ropey::Rope;

struct Document {
    rope: Rope,
}

impl Document {
    fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(&normalize(text)),
        }
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, insert);
    }

    fn paragraph_range(&self, byte: usize) -> Range<usize> {
        let byte = byte.min(self.rope.len_bytes());
        let line = self.rope.char_to_line(self.rope.byte_to_char(byte));
        let start = self.rope.line_to_byte(line);
        let end = if line + 1 < self.rope.len_lines() {
            self.rope.line_to_byte(line + 1)
        } else {
            self.rope.len_bytes()
        };
        if start == end && start > 0 && self.rope.byte(start - 1) == b'\n' {
            start - 1..end
        } else {
            start..end
        }
    }
}

impl WordBoundaries for Document {
    fn prev_word(&self, byte: usize) -> Option<usize> {
        let mut chars = self.rope.chars_at(self.rope.byte_to_char(byte));
        let mut offset = byte;
        let mut in_word = false;
        while let Some(character) = chars.prev() {
            if in_word && !character.is_alphanumeric() && character != '_' {
                break;
            }
            if character.is_alphanumeric() || character == '_' {
                in_word = true;
            }
            offset -= character.len_utf8();
            if in_word && offset == 0 {
                break;
            }
        }
        Some(offset)
    }

    fn next_word(&self, byte: usize) -> Option<usize> {
        let mut offset = byte;
        let mut in_word = false;
        for character in self.rope.chars_at(self.rope.byte_to_char(byte)) {
            if in_word && !(character.is_alphanumeric() || character == '_') {
                break;
            }
            if character.is_alphanumeric() || character == '_' {
                in_word = true;
            }
            offset += character.len_utf8();
        }
        Some(offset)
    }
}

pub struct TextEdit {
    document: Document,
    caret: Caret,
    anchor: Option<usize>,
    goal: Option<f32>,
}

impl TextEdit {
    pub fn new(content: &str) -> Self {
        let document = Document::new(content);
        Self {
            caret: Caret {
                byte_index: document.rope.len_bytes(),
                line_index: usize::MAX,
            },
            document,
            anchor: None,
            goal: None,
        }
    }

    pub fn content(&self) -> String {
        self.document.rope.to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.document.rope.len_bytes() == 0
    }

    pub fn caret(&self) -> Caret {
        self.caret
    }

    pub fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        let start = anchor.min(self.caret.byte_index);
        let end = anchor.max(self.caret.byte_index);
        (start != end).then_some(start..end)
    }

    pub fn place(&mut self, caret: Caret, select: bool) {
        self.set_caret(caret, select);
        self.goal = None;
    }

    pub fn motion(&mut self, layout: &Layout, motion: Motion, select: bool) {
        let caret = layout.caret_move(
            self.caret,
            motion,
            &mut self.goal,
            &self.document,
        );
        self.set_caret(caret, select);
    }

    pub fn select_word(&mut self, layout: &Layout, byte: usize) {
        self.select_range(layout, layout.select_word_at(byte, &self.document));
    }

    pub fn select_paragraph(&mut self, layout: &Layout, byte: usize) {
        self.select_range(layout, self.document.paragraph_range(byte));
    }

    pub fn insert(&mut self, text: &str) {
        let byte = self.caret.byte_index;
        let range = self.selection().unwrap_or(byte..byte);
        self.replace(range, text);
    }

    pub fn insert_normalized(&mut self, text: &str) {
        self.insert(&normalize(text));
    }

    pub fn backspace(&mut self, layout: &Layout) -> bool {
        let byte = self.caret.byte_index;
        let range = self
            .selection()
            .or_else(|| layout.prev_caret_stop(byte).map(|start| start..byte));
        let Some(range) = range else {
            return false;
        };
        self.replace(range, "");
        true
    }

    pub fn delete(&mut self, layout: &Layout) -> bool {
        let byte = self.caret.byte_index;
        let range = self
            .selection()
            .or_else(|| layout.next_caret_stop(byte).map(|end| byte..end));
        let Some(range) = range else {
            return false;
        };
        self.replace(range, "");
        true
    }

    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection() else {
            return false;
        };
        self.replace(range, "");
        true
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = Caret {
            byte_index: self.document.rope.len_bytes(),
            line_index: usize::MAX,
        };
        self.goal = None;
    }

    #[cfg(any(not(target_arch = "wasm32"), test))]
    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        let chars = self.document.rope.byte_to_char(range.start)
            ..self.document.rope.byte_to_char(range.end);
        Some(self.document.rope.slice(chars).to_string())
    }

    pub fn settle(&mut self, layout: &Layout) {
        self.caret = layout.caret_after_edit(self.caret.byte_index);
    }

    pub fn clamp(&mut self, layout: &Layout) {
        self.caret = layout.clamp_caret(self.caret);
    }

    fn set_caret(&mut self, caret: Caret, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret.byte_index);
            }
        } else {
            self.anchor = None;
        }
        self.caret = caret;
    }

    fn select_range(&mut self, layout: &Layout, range: Range<usize>) {
        self.anchor = Some(range.start);
        self.caret = layout.caret_at(range.end);
        self.goal = None;
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let at = range.start;
        self.document.replace(range, insert);
        self.caret.byte_index = at + insert.len();
        self.anchor = None;
        self.goal = None;
    }
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use based::widget::sanscale::raw::{CaretStop, LayoutLineSpec, LineMetrics};

    fn line(byte_range: Range<usize>, top_em: f32) -> LayoutLineSpec {
        let width_em = byte_range.len() as f32;
        let carets = (byte_range.start..=byte_range.end)
            .map(|byte_index| CaretStop {
                byte_index,
                x_em: (byte_index - byte_range.start) as f32,
            })
            .collect();
        LayoutLineSpec {
            byte_range,
            metrics: LineMetrics {
                top_em,
                baseline_em: top_em + 0.8,
                height_em: 1.0,
                width_em,
            },
            carets,
        }
    }

    #[test]
    fn word_selection_uses_rope_boundaries() {
        let layout = Layout::from_lines(vec![line(0..10, 0.0)]);
        let mut edit = TextEdit::new("alpha beta");

        edit.select_word(&layout, 2);

        assert_eq!(edit.selection(), Some(0..5));
        assert_eq!(edit.selected_text().as_deref(), Some("alpha"));
    }

    #[test]
    fn paragraph_selection_includes_the_hard_break() {
        let layout = Layout::from_lines(vec![
            line(0..5, 0.0),
            line(6..12, 1.0),
            line(13..18, 2.0),
        ]);
        let mut edit = TextEdit::new("first\nsecond\nthird");

        edit.select_paragraph(&layout, 8);

        assert_eq!(edit.selection(), Some(6..13));
        assert_eq!(edit.selected_text().as_deref(), Some("second\n"));
    }

    #[test]
    fn paragraph_selection_spans_soft_wrapped_lines() {
        let layout = Layout::from_lines(vec![
            line(0..2, 0.0),
            line(2..5, 1.0),
            line(6..12, 2.0),
        ]);
        let mut edit = TextEdit::new("first\nsecond");

        edit.select_paragraph(&layout, 3);

        assert_eq!(edit.selection(), Some(0..6));
        assert_eq!(edit.selected_text().as_deref(), Some("first\n"));
    }

    #[test]
    fn blank_paragraph_selection_selects_its_newline() {
        let layout = Layout::from_lines(vec![
            line(0..1, 0.0),
            line(2..2, 1.0),
            line(3..4, 2.0),
        ]);
        let mut edit = TextEdit::new("a\n\nb");

        edit.select_paragraph(&layout, 2);

        assert_eq!(edit.selection(), Some(2..3));
        assert_eq!(edit.selected_text().as_deref(), Some("\n"));

        let layout = Layout::from_lines(vec![line(0..1, 0.0), line(2..2, 1.0)]);
        let mut edit = TextEdit::new("a\n");
        edit.select_paragraph(&layout, 2);
        assert_eq!(edit.selection(), Some(1..2));
        assert_eq!(edit.selected_text().as_deref(), Some("\n"));
    }
}
