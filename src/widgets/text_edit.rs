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
