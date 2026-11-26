//! One-line text input.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Default)]
pub struct Input {
    text: String,
    /// Cursor position in chars.
    cursor: usize,
}

impl Input {
    pub fn new(s: impl Into<String>) -> Self {
        let text = s.into();
        let cursor = text.chars().count();
        Input { text, cursor }
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Input::default();
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, i: usize) -> usize {
        self.text
            .char_indices()
            .nth(i)
            .map_or(self.text.len(), |(b, _)| b)
    }

    /// Applies an editing key. Returns false if the key is not for the input.
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => self.clear(),
            KeyCode::Char(c) if !ctrl => {
                let b = self.byte(self.cursor);
                self.text.insert(b, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let b = self.byte(self.cursor);
                self.text.remove(b);
            }
            KeyCode::Delete if self.cursor < self.len() => {
                let b = self.byte(self.cursor);
                self.text.remove(b);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => return false,
        }
        true
    }

    /// The part that fits in `width` columns, and the cursor column inside it.
    pub fn view(&self, width: usize) -> (String, usize) {
        let width = width.max(1);
        let start = self.cursor.saturating_sub(width - 1);
        let s = self
            .text
            .chars()
            .skip(start)
            .take(width)
            .map(|c| if c == '\n' { '⏎' } else { c })
            .collect();
        (s, self.cursor - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_scrolls_and_shows_newlines() {
        let i = Input::new("ab\ncdef");
        assert_eq!(i.view(4), ("def".to_string(), 3));
        assert_eq!(Input::new("a\nb").view(10).0, "a⏎b");
    }
}
