//! Modal forms: a list of text and choice fields.

use super::Purpose;
use super::input::Input;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum Field {
    Text {
        label: &'static str,
        input: Input,
    },
    Choice {
        label: &'static str,
        options: Vec<String>,
        sel: usize,
    },
}

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Field::Text { label, .. } | Field::Choice { label, .. } => label,
        }
    }
}

pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub purpose: Purpose,
    pub error: Option<String>,
}

pub enum Outcome {
    Continue,
    Submit,
    Cancel,
}

impl Form {
    pub fn new(title: impl Into<String>, purpose: Purpose) -> Self {
        Form {
            title: title.into(),
            fields: vec![],
            focus: 0,
            purpose,
            error: None,
        }
    }

    pub fn with_text(mut self, label: &'static str, value: impl Into<String>) -> Self {
        self.fields.push(Field::Text {
            label,
            input: Input::new(value),
        });
        self
    }

    pub fn with_choice(mut self, label: &'static str, options: Vec<String>, sel: usize) -> Self {
        let sel = sel.min(options.len().saturating_sub(1));
        self.fields.push(Field::Choice {
            label,
            options,
            sel,
        });
        self
    }

    /// Text of a text field, or the selected option of a choice.
    pub fn get(&self, label: &str) -> String {
        match self.fields.iter().find(|f| f.label() == label) {
            Some(Field::Text { input, .. }) => input.value().trim().to_string(),
            Some(Field::Choice { options, sel, .. }) => {
                options.get(*sel).cloned().unwrap_or_default()
            }
            None => String::new(),
        }
    }

    pub fn sel(&self, label: &str) -> usize {
        match self.fields.iter().find(|f| f.label() == label) {
            Some(Field::Choice { sel, .. }) => *sel,
            _ => 0,
        }
    }

    pub fn input_mut(&mut self, label: &str) -> Option<&mut Input> {
        self.fields.iter_mut().find_map(|f| match f {
            Field::Text { label: l, input } if *l == label => Some(input),
            _ => None,
        })
    }

    pub fn focused_label(&self) -> &'static str {
        self.fields[self.focus].label()
    }

    pub fn handle(&mut self, key: KeyEvent) -> Outcome {
        let n = self.fields.len();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('s') if ctrl => return Outcome::Submit,
            KeyCode::Enter if self.focus + 1 == n => return Outcome::Submit,
            KeyCode::Enter | KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % n,
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + n - 1) % n,
            _ => match &mut self.fields[self.focus] {
                Field::Text { input, .. } => {
                    input.handle(key);
                }
                Field::Choice { options, sel, .. } => {
                    let m = options.len().max(1);
                    match key.code {
                        KeyCode::Left => *sel = (*sel + m - 1) % m,
                        KeyCode::Right | KeyCode::Char(' ') => *sel = (*sel + 1) % m,
                        _ => {}
                    }
                }
            },
        }
        Outcome::Continue
    }
}
