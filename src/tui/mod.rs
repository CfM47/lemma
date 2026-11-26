//! Full-screen editor.

mod form;
mod input;
mod ui;

use crate::model::{Family, Kind, Paper, Status, Symbol, TCS_SECTIONS, parse_list, valid_id};
use crate::outline::Outline;
use anyhow::Result;
use form::{Form, Outcome};
use input::Input;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Nodes,
    Alphabet,
    Outline,
    Checks,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Nodes, Tab::Alphabet, Tab::Outline, Tab::Checks];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Nodes => "Statements",
            Tab::Alphabet => "Alphabet",
            Tab::Outline => "Outline",
            Tab::Checks => "Checks",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AlphaFocus {
    Symbols,
    Families,
}

/// Choose a dependency of `target`, existing or new. In interview mode it walks
/// through every open statement asking "what does it depend on?".
pub struct Picker {
    pub target: String,
    pub input: Input,
    pub sel: Option<usize>,
    pub interview: bool,
    pub skipped: Vec<String>,
}

pub enum Purpose {
    PaperInfo {
        onboarding: bool,
    },
    NewNode {
        link_from: Option<String>,
        main: bool,
        resume: Option<Picker>,
        interview_after: bool,
    },
    EditNode(String),
    Known {
        resume: Picker,
    },
    NewSymbol,
    EditSymbol(usize),
    NewFamily,
    EditFamily(usize),
}

#[allow(clippy::enum_variant_names)]
pub enum Confirm {
    DeleteNode(String),
    DeleteSymbol(usize),
    DeleteFamily(usize),
}

pub enum Modal {
    Form(Form),
    Picker(Picker),
    Unlink { target: String, sel: usize },
    Confirm { message: String, action: Confirm },
    Help,
}

pub struct App {
    pub paper: Paper,
    path: PathBuf,
    pub tab: Tab,
    pub node_sel: usize,
    pub sym_sel: usize,
    pub fam_sel: usize,
    pub alpha_focus: AlphaFocus,
    pub scroll: u16,
    pub modal: Option<Modal>,
    pub status: String,
    quit: bool,
}

pub fn run(path: &Path) -> Result<()> {
    let onboarding = !path.exists();
    let paper = if onboarding {
        Paper::new()
    } else {
        Paper::load(path)?
    };
    let mut app = App {
        paper,
        path: path.to_path_buf(),
        tab: Tab::Nodes,
        node_sel: 0,
        sym_sel: 0,
        fam_sel: 0,
        alpha_focus: AlphaFocus::Symbols,
        scroll: 0,
        modal: None,
        status: String::new(),
        quit: false,
    };
    if onboarding {
        app.modal = Some(app.paper_form(true));
        app.status = format!("New plan, saved to {} on the first change.", path.display());
    } else if app.paper.nodes.is_empty() {
        app.modal = app.start_interview(None);
    }
    let mut terminal = ratatui::init();
    let res = app.event_loop(&mut terminal);
    ratatui::restore();
    res
}

impl App {
    fn event_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| ui::draw(f, self))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.on_key(key);
            }
        }
        Ok(())
    }

    fn save(&mut self) {
        if let Err(e) = self.paper.save(&self.path) {
            self.status = format!("Could not save: {e:#}");
        }
    }

    /// Statements in reading order (file order while there is a cycle).
    pub fn order(&self) -> Vec<usize> {
        Outline::build(&self.paper)
            .map(|o| o.order)
            .unwrap_or_else(|_| (0..self.paper.nodes.len()).collect())
    }

    pub fn selected_id(&self) -> Option<String> {
        self.order()
            .get(self.node_sel)
            .map(|&v| self.paper.nodes[v].id.clone())
    }

    fn select(&mut self, id: &str) {
        if let Some(v) = self.paper.index(id)
            && let Some(pos) = self.order().iter().position(|&x| x == v)
        {
            self.node_sel = pos;
        }
    }

    fn clamp(&mut self) {
        let last = |n: usize| n.saturating_sub(1);
        self.node_sel = self.node_sel.min(last(self.paper.nodes.len()));
        self.sym_sel = self.sym_sel.min(last(self.paper.symbols.len()));
        self.fam_sel = self.fam_sel.min(last(self.paper.families.len()));
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some(modal) = self.modal.take() {
            self.modal = self.on_modal(modal, key);
            self.clamp();
            return;
        }
        self.status.clear();
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char(c @ '1'..='4') => {
                self.tab = Tab::ALL[c as usize - '1' as usize];
                self.scroll = 0;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let i = Tab::ALL.iter().position(|&t| t == self.tab).unwrap();
                let step = if key.code == KeyCode::Tab { 1 } else { 3 };
                self.tab = Tab::ALL[(i + step) % 4];
                self.scroll = 0;
            }
            KeyCode::Char('P') => self.modal = Some(self.paper_form(false)),
            _ => match self.tab {
                Tab::Nodes => self.on_nodes(key),
                Tab::Alphabet => self.on_alphabet(key),
                Tab::Outline | Tab::Checks => self.on_scroll(key),
            },
        }
        self.clamp();
    }

    fn on_scroll(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll = self.scroll.saturating_add(15),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(15),
            KeyCode::Char('g') | KeyCode::Home => self.scroll = 0,
            _ => {}
        }
    }

    fn on_nodes(&mut self, key: KeyEvent) {
        let id = self.selected_id();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.node_sel += 1,
            KeyCode::Char('k') | KeyCode::Up => self.node_sel = self.node_sel.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.node_sel = 0,
            KeyCode::Char('G') | KeyCode::End => self.node_sel = usize::MAX,
            KeyCode::Char('a') => {
                let main = !self.paper.nodes.iter().any(|n| n.main);
                let purpose = Purpose::NewNode {
                    link_from: None,
                    main,
                    resume: None,
                    interview_after: false,
                };
                let kind = if main { Kind::Theorem } else { Kind::Lemma };
                let heading = if main {
                    "What is your main result?"
                } else {
                    "New statement"
                };
                self.modal = Some(self.node_form(heading, "", kind, purpose));
            }
            KeyCode::Char('A') => {
                let purpose = Purpose::NewNode {
                    link_from: None,
                    main: true,
                    resume: None,
                    interview_after: true,
                };
                self.modal = Some(self.node_form("New main result", "", Kind::Theorem, purpose));
            }
            KeyCode::Char('i') => self.modal = self.start_interview(None),
            KeyCode::Enter => self.modal = self.start_interview(id),
            _ => {}
        }
        let Some(id) = self.selected_id() else { return };
        match key.code {
            KeyCode::Char('e') => self.modal = Some(self.edit_form(&id)),
            KeyCode::Char('d') => {
                self.modal = Some(Modal::Confirm {
                    message: format!(
                        "Delete {}? Statements that use it forget it.",
                        self.paper.label(&id)
                    ),
                    action: Confirm::DeleteNode(id.clone()),
                })
            }
            KeyCode::Char('u') => {
                self.modal = Some(Modal::Picker(Picker {
                    target: id.clone(),
                    input: Input::default(),
                    sel: None,
                    interview: false,
                    skipped: vec![],
                }))
            }
            KeyCode::Char('x') => {
                if self.paper.node(&id).is_some_and(|n| !n.uses.is_empty()) {
                    self.modal = Some(Modal::Unlink {
                        target: id.clone(),
                        sel: 0,
                    });
                } else {
                    self.status = "It uses nothing.".into();
                }
            }
            KeyCode::Char('m') => {
                let n = self.paper.node_mut(&id).unwrap();
                n.main = !n.main;
                let msg = if n.main {
                    "Marked as a main result."
                } else {
                    "No longer a main result."
                };
                self.status = msg.into();
                self.save();
            }
            KeyCode::Char('p') => {
                let n = self.paper.node_mut(&id).unwrap();
                n.appendix = !n.appendix;
                let msg = if n.appendix {
                    "Proof deferred to the appendix."
                } else {
                    "Proof back in the body."
                };
                self.status = msg.into();
                self.save();
            }
            KeyCode::Char('b') => {
                let n = self.paper.node_mut(&id).unwrap();
                n.status = if n.status == Status::Basic {
                    Status::Open
                } else {
                    Status::Basic
                };
                let msg = if n.status == Status::Basic {
                    "Marked basic: nothing more to expand."
                } else {
                    "Open again."
                };
                self.status = msg.into();
                self.save();
            }
            KeyCode::Char('s') => self.modal = Some(self.symbol_form(None, Some(&id))),
            _ => {}
        }
        self.select_keep(id);
    }

    /// After an edit the reading order may change: keep the same statement selected.
    fn select_keep(&mut self, id: String) {
        if self.modal.is_none()
            && self.paper.index(&id).is_some()
            && self.selected_id().as_deref() != Some(&id)
        {
            self.select(&id);
        }
    }

    fn on_alphabet(&mut self, key: KeyEvent) {
        let symbols = self.alpha_focus == AlphaFocus::Symbols;
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                self.alpha_focus = if symbols {
                    AlphaFocus::Families
                } else {
                    AlphaFocus::Symbols
                }
            }
            KeyCode::Char('j') | KeyCode::Down if symbols => self.sym_sel += 1,
            KeyCode::Char('k') | KeyCode::Up if symbols => {
                self.sym_sel = self.sym_sel.saturating_sub(1)
            }
            KeyCode::Char('j') | KeyCode::Down => self.fam_sel += 1,
            KeyCode::Char('k') | KeyCode::Up => self.fam_sel = self.fam_sel.saturating_sub(1),
            KeyCode::Char('a') => self.modal = Some(self.symbol_form(None, None)),
            KeyCode::Char('f') => self.modal = Some(self.family_form(None)),
            KeyCode::Char('e') if symbols && !self.paper.symbols.is_empty() => {
                self.modal = Some(self.symbol_form(Some(self.sym_sel), None))
            }
            KeyCode::Char('e') if !symbols && !self.paper.families.is_empty() => {
                self.modal = Some(self.family_form(Some(self.fam_sel)))
            }
            KeyCode::Char('d') if symbols && !self.paper.symbols.is_empty() => {
                let s = &self.paper.symbols[self.sym_sel];
                self.modal = Some(Modal::Confirm {
                    message: format!("Delete ${}$ ({})?", s.notation, s.meaning),
                    action: Confirm::DeleteSymbol(self.sym_sel),
                })
            }
            KeyCode::Char('d') if !symbols && !self.paper.families.is_empty() => {
                let f = &self.paper.families[self.fam_sel];
                self.modal = Some(Modal::Confirm {
                    message: format!("Delete the family “{}”?", f.name),
                    action: Confirm::DeleteFamily(self.fam_sel),
                })
            }
            _ => {}
        }
    }

    fn on_modal(&mut self, modal: Modal, key: KeyEvent) -> Option<Modal> {
        match modal {
            Modal::Help => None,
            Modal::Confirm { message, action } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    match action {
                        Confirm::DeleteNode(id) => {
                            self.status = format!("Deleted {}.", self.paper.label(&id));
                            self.paper.remove_node(&id);
                        }
                        Confirm::DeleteSymbol(i) => {
                            self.paper.symbols.remove(i);
                        }
                        Confirm::DeleteFamily(i) => {
                            self.paper.families.remove(i);
                        }
                    }
                    self.save();
                    None
                }
                KeyCode::Char('n') | KeyCode::Esc => None,
                _ => Some(Modal::Confirm { message, action }),
            },
            Modal::Unlink { target, mut sel } => {
                let uses = self
                    .paper
                    .node(&target)
                    .map(|n| n.uses.clone())
                    .unwrap_or_default();
                match key.code {
                    KeyCode::Esc => return None,
                    KeyCode::Down | KeyCode::Char('j') => {
                        sel = (sel + 1).min(uses.len().saturating_sub(1))
                    }
                    KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
                    KeyCode::Enter | KeyCode::Char('x') => {
                        if let Some(u) = uses.get(sel) {
                            self.status = format!(
                                "{} no longer uses {}.",
                                self.paper.label(&target),
                                self.paper.label(u)
                            );
                            self.paper.unlink(&target, u);
                            self.save();
                        }
                        if uses.len() <= 1 {
                            return None;
                        }
                        sel = sel.min(uses.len() - 2);
                    }
                    _ => {}
                }
                Some(Modal::Unlink { target, sel })
            }
            Modal::Picker(p) => self.on_picker(p, key),
            Modal::Form(mut form) => {
                let outcome = form.handle(key);
                match outcome {
                    Outcome::Continue => {
                        self.prefill_notation(&mut form);
                        Some(Modal::Form(form))
                    }
                    Outcome::Cancel => match form.purpose {
                        Purpose::NewNode {
                            resume: Some(p), ..
                        }
                        | Purpose::Known { resume: p } => Some(Modal::Picker(p)),
                        _ => None,
                    },
                    Outcome::Submit => self.submit(form),
                }
            }
        }
    }

    /// When the cursor reaches an empty Notation field, offer the first free letter of the family.
    fn prefill_notation(&self, form: &mut Form) {
        if !matches!(form.purpose, Purpose::NewSymbol) || form.focused_label() != "Notation" {
            return;
        }
        let family = form.get("Family");
        if let Some(letter) = self.paper.suggest_letter(&family)
            && let Some(input) = form.input_mut("Notation")
            && input.is_empty()
        {
            *input = Input::new(letter);
        }
    }

    // ----- interview -----

    fn start_interview(&mut self, from: Option<String>) -> Option<Modal> {
        if !self.paper.nodes.iter().any(|n| n.main) {
            let purpose = Purpose::NewNode {
                link_from: None,
                main: true,
                resume: None,
                interview_after: true,
            };
            return Some(self.node_form("What is your main result?", "", Kind::Theorem, purpose));
        }
        let from = from.filter(|id| self.paper.node(id).is_some_and(|n| n.kind.expandable()));
        match from.or_else(|| self.paper.open_queue().into_iter().next()) {
            Some(target) => {
                self.select(&target);
                Some(Modal::Picker(Picker {
                    target,
                    input: Input::default(),
                    sel: None,
                    interview: true,
                    skipped: vec![],
                }))
            }
            None => {
                self.status = "Nothing left to expand. Add a result with a/A, or press Enter on a statement to revisit it.".into();
                None
            }
        }
    }

    fn next_interview(&mut self, mut p: Picker) -> Option<Modal> {
        let next = self
            .paper
            .open_queue()
            .into_iter()
            .find(|id| !p.skipped.contains(id));
        match next {
            Some(id) => {
                self.select(&id);
                p.target = id;
                p.input.clear();
                p.sel = None;
                Some(Modal::Picker(p))
            }
            None => {
                self.status = if p.skipped.is_empty() {
                    "Every statement is expanded. See the outline with 3.".into()
                } else {
                    format!(
                        "Interview over; {} skipped statement(s) still open.",
                        p.skipped.len()
                    )
                };
                None
            }
        }
    }

    /// Statements `target` may start using: not itself, not already used, no cycle.
    pub fn candidates(&self, p: &Picker) -> Vec<usize> {
        let q = p.input.value().trim().to_lowercase();
        let uses = self
            .paper
            .node(&p.target)
            .map(|n| n.uses.clone())
            .unwrap_or_default();
        self.order()
            .into_iter()
            .filter(|&v| {
                let n = &self.paper.nodes[v];
                n.id != p.target
                    && !uses.contains(&n.id)
                    && !self.paper.depends_on(&n.id, &p.target)
                    && (q.is_empty() || n.title.to_lowercase().contains(&q) || n.id.contains(&q))
            })
            .collect()
    }

    fn set_status(&mut self, id: &str, status: Status) {
        if let Some(n) = self.paper.node_mut(id) {
            n.status = status;
        }
        self.save();
    }

    fn on_picker(&mut self, mut p: Picker, key: KeyEvent) -> Option<Modal> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let cands = self.candidates(&p);
        match key.code {
            KeyCode::Esc => {
                if p.interview {
                    self.status = "Interview paused: i resumes it.".into();
                }
                return None;
            }
            KeyCode::Up => p.sel = p.sel.and_then(|i| i.checked_sub(1)),
            KeyCode::Down if !cands.is_empty() => {
                p.sel = Some(p.sel.map_or(0, |i| (i + 1).min(cands.len() - 1)))
            }
            KeyCode::Enter => {
                let typed = p.input.value().trim().to_string();
                let exact = cands
                    .iter()
                    .position(|&v| self.paper.nodes[v].title.eq_ignore_ascii_case(&typed));
                if let Some(i) = p.sel.or(exact)
                    && let Some(&v) = cands.get(i)
                {
                    let id = self.paper.nodes[v].id.clone();
                    match self.paper.link(&p.target, &id) {
                        Ok(()) => {
                            self.status = format!(
                                "{} uses {}.",
                                self.paper.label(&p.target),
                                self.paper.label(&id)
                            );
                            self.save();
                        }
                        Err(e) => self.status = e,
                    }
                    p.input.clear();
                    p.sel = None;
                } else if !typed.is_empty() {
                    let heading = format!("New statement used by {}", self.paper.label(&p.target));
                    let link_from = Some(p.target.clone());
                    let purpose = Purpose::NewNode {
                        link_from,
                        main: false,
                        resume: Some(p),
                        interview_after: false,
                    };
                    return Some(self.node_form(&heading, &typed, Kind::Lemma, purpose));
                } else if p.interview {
                    let target = p.target.clone();
                    self.set_status(&target, Status::Expanded);
                    return self.next_interview(p);
                } else {
                    return None;
                }
            }
            KeyCode::Char('b') if ctrl && p.interview => {
                let target = p.target.clone();
                self.set_status(&target, Status::Basic);
                self.status = format!("{} is basic.", self.paper.label(&target));
                return self.next_interview(p);
            }
            KeyCode::Char('k') if ctrl && p.interview => {
                let heading = format!(
                    "{} is a known result: where is it from?",
                    self.paper.label(&p.target)
                );
                let cite = self
                    .paper
                    .node(&p.target)
                    .map(|n| n.cite.clone())
                    .unwrap_or_default();
                return Some(Modal::Form(
                    Form::new(heading, Purpose::Known { resume: p }).with_text("Cite", cite),
                ));
            }
            KeyCode::Char('n') if ctrl && p.interview => {
                p.skipped.push(p.target.clone());
                return self.next_interview(p);
            }
            _ => {
                if p.input.handle(key) {
                    p.sel = None;
                }
            }
        }
        Some(Modal::Picker(p))
    }

    // ----- forms -----

    fn paper_form(&self, onboarding: bool) -> Modal {
        let mut sections = vec!["(undecided)".to_string()];
        sections.extend(TCS_SECTIONS.iter().map(|s| s.to_string()));
        let sel = TCS_SECTIONS
            .iter()
            .position(|s| *s == self.paper.paper.section)
            .map_or(0, |i| i + 1);
        let heading = if onboarding { "New paper" } else { "Paper" };
        Modal::Form(
            Form::new(heading, Purpose::PaperInfo { onboarding })
                .with_text("Title", self.paper.paper.title.clone())
                .with_text("Story", self.paper.paper.story.clone())
                .with_choice("TCS section", sections, sel),
        )
    }

    fn node_form(&self, heading: &str, title: &str, kind: Kind, purpose: Purpose) -> Modal {
        let kinds = Kind::ALL.iter().map(|k| k.name().to_string()).collect();
        let k = Kind::ALL.iter().position(|&x| x == kind).unwrap();
        Modal::Form(
            Form::new(heading, purpose)
                .with_text("Title", title)
                .with_choice("Kind", kinds, k)
                .with_text("Statement", "")
                .with_text("Cite", ""),
        )
    }

    fn edit_form(&self, id: &str) -> Modal {
        let n = self.paper.node(id).unwrap();
        let kinds = Kind::ALL.iter().map(|k| k.name().to_string()).collect();
        let k = Kind::ALL.iter().position(|&x| x == n.kind).unwrap();
        Modal::Form(
            Form::new(
                format!("Edit {}", self.paper.label(id)),
                Purpose::EditNode(id.to_string()),
            )
            .with_text("Title", n.title.clone())
            .with_choice("Kind", kinds, k)
            .with_text("Statement", n.statement.clone())
            .with_text("Cite", n.cite.clone())
            .with_text("Mentions", n.symbols.join(" "))
            .with_text("Id", n.id.clone()),
        )
    }

    /// Introducer choices: general notation, then every statement in reading order.
    fn introducers(&self) -> Vec<String> {
        self.order()
            .into_iter()
            .map(|v| self.paper.nodes[v].id.clone())
            .collect()
    }

    fn symbol_form(&self, edit: Option<usize>, introducer: Option<&str>) -> Modal {
        let sym = edit.map(|i| self.paper.symbols[i].clone());
        let mut families = vec!["(none)".to_string()];
        families.extend(self.paper.families.iter().map(|f| f.name.clone()));
        let fam_sel = sym
            .as_ref()
            .and_then(|s| self.paper.families.iter().position(|f| f.name == s.family))
            .map_or(0, |i| i + 1);
        let ids = self.introducers();
        let mut intro = vec!["(general notation)".to_string()];
        intro.extend(ids.iter().map(|id| self.paper.label(id)));
        let wanted = sym
            .as_ref()
            .map(|s| s.introduced_by.as_str())
            .or(introducer);
        let intro_sel = wanted
            .and_then(|w| ids.iter().position(|id| id == w))
            .map_or(0, |i| i + 1);
        let (heading, purpose) = match edit {
            Some(i) => ("Edit symbol", Purpose::EditSymbol(i)),
            None => ("New symbol", Purpose::NewSymbol),
        };
        let sym = sym.unwrap_or(Symbol {
            notation: String::new(),
            meaning: String::new(),
            family: String::new(),
            introduced_by: String::new(),
            macro_name: String::new(),
        });
        Modal::Form(
            Form::new(heading, purpose)
                .with_choice("Family", families, fam_sel)
                .with_text("Notation", sym.notation)
                .with_text("Meaning", sym.meaning)
                .with_choice("Introduced by", intro, intro_sel)
                .with_text("Macro", sym.macro_name),
        )
    }

    fn family_form(&self, edit: Option<usize>) -> Modal {
        let (heading, purpose, name, letters) = match edit {
            Some(i) => {
                let f = &self.paper.families[i];
                (
                    "Edit family",
                    Purpose::EditFamily(i),
                    f.name.clone(),
                    f.letters.join(" "),
                )
            }
            None => (
                "New family of letters",
                Purpose::NewFamily,
                String::new(),
                String::new(),
            ),
        };
        Modal::Form(
            Form::new(heading, purpose)
                .with_text("Name", name)
                .with_text("Letters", letters),
        )
    }

    fn validate(&self, form: &Form) -> Result<(), String> {
        match &form.purpose {
            Purpose::NewNode { .. } | Purpose::EditNode(_) if form.get("Title").is_empty() => {
                Err("Give it a short title.".into())
            }
            Purpose::EditNode(old) => {
                let id = form.get("Id");
                if &id != old && !valid_id(&id) {
                    Err("Ids use letters, digits and : - _ .".into())
                } else if &id != old && self.paper.index(&id).is_some() {
                    Err(format!("Id “{id}” is taken."))
                } else {
                    Ok(())
                }
            }
            Purpose::NewSymbol | Purpose::EditSymbol(_) => {
                let notation = form.get("Notation");
                let this = match form.purpose {
                    Purpose::EditSymbol(i) => Some(i),
                    _ => None,
                };
                let macro_name = form.get("Macro");
                if notation.is_empty() {
                    Err("Notation is empty.".into())
                } else if let Some(other) = self
                    .paper
                    .symbols
                    .iter()
                    .enumerate()
                    .find(|(i, s)| Some(*i) != this && s.notation == notation)
                {
                    Err(format!(
                        "${notation}$ already means “{}”: one letter, one meaning.",
                        other.1.meaning
                    ))
                } else if !macro_name.is_empty()
                    && !(macro_name.starts_with('\\')
                        && macro_name[1..].chars().all(|c| c.is_ascii_alphabetic())
                        && macro_name.len() > 1)
                {
                    Err("A macro looks like \\graph (backslash and letters).".into())
                } else {
                    Ok(())
                }
            }
            Purpose::NewFamily | Purpose::EditFamily(_) => {
                let name = form.get("Name");
                let this = match form.purpose {
                    Purpose::EditFamily(i) => Some(i),
                    _ => None,
                };
                if name.is_empty() {
                    Err("Name the family (graph, integer, set...).".into())
                } else if self
                    .paper
                    .families
                    .iter()
                    .enumerate()
                    .any(|(i, f)| Some(i) != this && f.name == name)
                {
                    Err(format!("There is already a family “{name}”."))
                } else if parse_list(&form.get("Letters")).is_empty() {
                    Err("List its letters, e.g. G H.".into())
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    fn submit(&mut self, mut form: Form) -> Option<Modal> {
        if let Err(e) = self.validate(&form) {
            form.error = Some(e);
            return Some(Modal::Form(form));
        }
        let purpose = std::mem::replace(&mut form.purpose, Purpose::NewFamily);
        match purpose {
            Purpose::PaperInfo { onboarding } => {
                self.paper.paper.title = form.get("Title");
                self.paper.paper.story = form.get("Story");
                let s = form.sel("TCS section");
                self.paper.paper.section = if s == 0 {
                    String::new()
                } else {
                    TCS_SECTIONS[s - 1].to_string()
                };
                self.save();
                if onboarding {
                    return self.start_interview(None);
                }
                None
            }
            Purpose::NewNode {
                link_from,
                main,
                resume,
                interview_after,
            } => {
                let kind = Kind::ALL[form.sel("Kind")];
                let id = self.paper.add_node(kind, &form.get("Title"));
                let n = self.paper.node_mut(&id).unwrap();
                n.statement = form.get("Statement");
                n.cite = form.get("Cite");
                n.main = main;
                if let Some(from) = link_from
                    && let Err(e) = self.paper.link(&from, &id)
                {
                    self.status = e;
                }
                self.save();
                self.select(&id);
                self.status = format!("Added {}.", self.paper.label(&id));
                if let Some(mut p) = resume {
                    self.select(&p.target.clone());
                    p.input.clear();
                    p.sel = None;
                    return Some(Modal::Picker(p));
                }
                if interview_after {
                    return self.start_interview(Some(id));
                }
                None
            }
            Purpose::EditNode(old) => {
                let id = form.get("Id");
                if let Err(e) = self.paper.rename_node(&old, &id) {
                    self.status = e;
                    return None;
                }
                let kind = Kind::ALL[form.sel("Kind")];
                // A hand-written id is kept; an automatic one follows the kind.
                let id = if id == old {
                    self.paper.set_kind(&id, kind)
                } else {
                    self.paper.node_mut(&id).unwrap().kind = kind;
                    id
                };
                let n = self.paper.node_mut(&id).unwrap();
                n.title = form.get("Title");
                n.statement = form.get("Statement");
                n.cite = form.get("Cite");
                n.symbols = parse_list(&form.get("Mentions"));
                if !n.kind.expandable() {
                    n.status = Status::Basic;
                }
                self.save();
                self.select(&id);
                None
            }
            Purpose::Known { resume } => {
                let id = self.paper.set_kind(&resume.target, Kind::Known);
                self.paper.node_mut(&id).unwrap().cite = form.get("Cite");
                self.save();
                self.status = format!("{} is a known result.", self.paper.label(&id));
                self.next_interview(resume)
            }
            Purpose::NewSymbol | Purpose::EditSymbol(_) => {
                let fam = form.sel("Family");
                let intro = form.sel("Introduced by");
                let sym = Symbol {
                    notation: form.get("Notation"),
                    meaning: form.get("Meaning"),
                    family: if fam == 0 {
                        String::new()
                    } else {
                        self.paper.families[fam - 1].name.clone()
                    },
                    introduced_by: if intro == 0 {
                        String::new()
                    } else {
                        self.introducers()[intro - 1].clone()
                    },
                    macro_name: form.get("Macro"),
                };
                self.status = format!("${}$: {}", sym.notation, sym.meaning);
                match purpose {
                    Purpose::EditSymbol(i) => self.paper.symbols[i] = sym,
                    _ => {
                        self.paper.symbols.push(sym);
                        self.sym_sel = self.paper.symbols.len() - 1;
                    }
                }
                self.save();
                None
            }
            Purpose::NewFamily | Purpose::EditFamily(_) => {
                let fam = Family {
                    name: form.get("Name"),
                    letters: parse_list(&form.get("Letters")),
                };
                match purpose {
                    Purpose::EditFamily(i) => {
                        let old = std::mem::replace(&mut self.paper.families[i], fam);
                        let new = self.paper.families[i].name.clone();
                        for s in &mut self.paper.symbols {
                            if s.family == old.name {
                                s.family = new.clone();
                            }
                        }
                    }
                    _ => {
                        self.paper.families.push(fam);
                        self.fam_sel = self.paper.families.len() - 1;
                    }
                }
                self.save();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app(path: PathBuf) -> App {
        App {
            paper: Paper::new(),
            path,
            tab: Tab::Nodes,
            node_sel: 0,
            sym_sel: 0,
            fam_sel: 0,
            alpha_focus: AlphaFocus::Symbols,
            scroll: 0,
            modal: None,
            status: String::new(),
            quit: false,
        }
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn ctrl(app: &mut App, c: char) {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
    }

    fn typ(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    fn screen(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(110, 32)).unwrap();
        t.draw(|f| ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn interview_builds_the_plan() {
        let dir = std::env::temp_dir().join(format!("lemma-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lemma.toml");
        let mut app = app(path.clone());
        app.modal = Some(app.paper_form(true));
        typ(&mut app, "On width");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // submit paper form, ask for the main result
        typ(&mut app, "Linear-time recognition");
        ctrl(&mut app, 's'); // main theorem created, interview starts on it
        assert!(screen(&app).contains("What does Theorem “Linear-time recognition” depend on?"));

        typ(&mut app, "Compression");
        press(&mut app, KeyCode::Enter); // new statement form, prefilled
        ctrl(&mut app, 's'); // lemma created, back to the interview
        typ(&mut app, "Width");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Right); // Lemma -> Corollary
        press(&mut app, KeyCode::Right); // -> Definition
        ctrl(&mut app, 's');
        typ(&mut app, "Courcelle");
        press(&mut app, KeyCode::Enter);
        ctrl(&mut app, 's');
        press(&mut app, KeyCode::Enter); // done with the theorem: next open is Compression
        let s = screen(&app);
        assert!(
            s.contains("What does Lemma “Compression” depend on?"),
            "{s}"
        );

        typ(&mut app, "Degree bound");
        press(&mut app, KeyCode::Enter);
        ctrl(&mut app, 's');
        press(&mut app, KeyCode::Down); // pick existing: Width
        let s = screen(&app);
        assert!(s.contains("▶ Def   Width"), "{s}");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // done with Compression
        // Next open: Width (BFS order), then Courcelle, then Degree bound.
        assert!(screen(&app).contains("depend on?"));
        ctrl(&mut app, 'b'); // Width is basic
        ctrl(&mut app, 'k'); // Courcelle is known
        typ(&mut app, "Theorem 1 of [3]");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // Degree bound: done, nothing more
        assert!(app.modal.is_none());
        assert!(
            app.status.contains("Every statement is expanded"),
            "{}",
            app.status
        );

        let saved = Paper::load(&path).unwrap();
        assert_eq!(saved.nodes.len(), 5);
        assert!(saved.open_queue().is_empty());
        let known = saved.nodes.iter().find(|n| n.title == "Courcelle").unwrap();
        assert_eq!(known.kind, Kind::Known);
        assert_eq!(known.cite, "Theorem 1 of [3]");
        assert_eq!(known.id, "thm:courcelle");
        assert!(crate::export::markdown(&saved).contains("## 3. Compression"));

        // Alphabet: the symbol form offers the first free graph letter.
        press(&mut app, KeyCode::Char('2'));
        press(&mut app, KeyCode::Char('a'));
        for _ in 0..8 {
            press(&mut app, KeyCode::Right); // (none) -> ... -> graph
        }
        press(&mut app, KeyCode::Enter);
        let Some(Modal::Form(f)) = &app.modal else {
            panic!()
        };
        assert_eq!(f.get("Family"), "graph");
        assert_eq!(f.get("Notation"), "G");
        press(&mut app, KeyCode::Enter);
        typ(&mut app, "input graph");
        ctrl(&mut app, 's');
        assert_eq!(app.paper.symbols[0].notation, "G");

        press(&mut app, KeyCode::Char('3'));
        assert!(screen(&app).contains("## 3. Compression"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
