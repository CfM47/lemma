//! Drawing.

use super::form::{Field, Form};
use super::{AlphaFocus, App, Modal, Picker, Tab};
use crate::checks::{self, Level, decorates};
use crate::export;
use crate::model::{Kind, Status};
use crate::outline::Outline;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

const ACCENT: Color = Color::Cyan;

pub fn draw(f: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());
    draw_header(f, header, app);
    match app.tab {
        Tab::Nodes => draw_nodes(f, body, app),
        Tab::Alphabet => draw_alphabet(f, body, app),
        Tab::Outline => draw_outline(f, body, app),
        Tab::Checks => draw_checks(f, body, app),
    }
    draw_footer(f, footer, app);
    if let Some(m) = &app.modal {
        draw_modal(f, m, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let title = if app.paper.paper.title.is_empty() {
        "Untitled paper"
    } else {
        &app.paper.paper.title
    };
    let mut spans = vec![
        Span::styled(" lemma ", Style::new().bg(ACCENT).fg(Color::Black).bold()),
        Span::raw(" "),
    ];
    for (i, t) in Tab::ALL.iter().enumerate() {
        let text = format!(" {} {} ", i + 1, t.title());
        spans.push(if *t == app.tab {
            Span::styled(text, Style::new().reversed().bold())
        } else {
            Span::styled(text, Style::new().dim())
        });
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(title.to_string(), Style::new().italic()));
    f.render_widget(Line::from(spans), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let hints = match (&app.modal, app.tab) {
        (Some(_), _) => "",
        (None, Tab::Nodes) => {
            "i interview · Enter expand · a add · A main · u use · x unuse · e edit · m main · p appendix · b basic · s symbol · d delete · ? help"
        }
        (None, Tab::Alphabet) => {
            "←→ families/symbols · a symbol · f family · e edit · d delete · ? help"
        }
        (None, _) => "j/k scroll · PgUp/PgDn · P paper · q quit · ? help",
    };
    let line = if app.status.is_empty() {
        Line::styled(format!(" {hints}"), Style::new().dim())
    } else {
        Line::styled(format!(" {}", app.status), Style::new().fg(Color::Yellow))
    };
    f.render_widget(line, area);
}

fn block(title: &str, focused: bool) -> Block<'static> {
    let b = Block::bordered().title(format!(" {title} "));
    if focused {
        b.border_style(Style::new().fg(ACCENT))
    } else {
        b.border_style(Style::new().dim())
    }
}

fn status_marker(kind: Kind, status: Status) -> Span<'static> {
    match (kind, status) {
        (Kind::Known, _) => Span::styled("§", Style::new().fg(Color::Blue)),
        (_, Status::Open) => Span::styled("?", Style::new().fg(Color::Yellow).bold()),
        (_, Status::Expanded) => Span::styled("✓", Style::new().fg(Color::Green)),
        (_, Status::Basic) => Span::styled("·", Style::new().dim()),
    }
}

fn draw_nodes(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);
    let p = &app.paper;
    let outline = Outline::build(p);
    let order = app.order();

    if p.nodes.is_empty() {
        let msg = Paragraph::new(vec![
            Line::raw(""),
            Line::raw("  No statements yet."),
            Line::raw(""),
            Line::from(vec![
                Span::raw("  Press "),
                Span::styled("i", Style::new().bold()),
                Span::raw(" to start with your main result."),
            ]),
        ])
        .block(block("Statements", true));
        f.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = order
        .iter()
        .map(|&v| {
            let n = &p.nodes[v];
            let num = outline
                .as_ref()
                .map_or("-".to_string(), |o| o.numbers[v].clone());
            let star = if n.main {
                Span::styled("★", Style::new().fg(Color::Yellow))
            } else {
                Span::raw(" ")
            };
            let title = if n.main {
                Span::styled(n.title.clone(), Style::new().bold())
            } else {
                Span::raw(n.title.clone())
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{num:>5} "), Style::new().dim()),
                star,
                status_marker(n.kind, n.status),
                Span::raw(" "),
                Span::styled(format!("{:<6}", n.kind.short()), Style::new().fg(ACCENT)),
                title,
            ]))
        })
        .collect();
    let open = p.open_queue().len();
    let title = format!("Statements · {} · {} open", p.nodes.len(), open);
    let list = List::new(items)
        .block(block(&title, true))
        .highlight_style(Style::new().bg(Color::DarkGray));
    let mut state = ListState::default().with_selected(Some(app.node_sel));
    f.render_stateful_widget(list, left, &mut state);

    let Some(&v) = order.get(app.node_sel) else {
        return;
    };
    let n = &p.nodes[v];
    let mut lines = vec![];
    let num = outline
        .as_ref()
        .map_or(String::new(), |o| format!(" {}", o.numbers[v]));
    lines.push(Line::from(vec![
        Span::styled(
            format!("{}{num}  ", n.kind.printed()),
            Style::new().fg(ACCENT).bold(),
        ),
        Span::styled(n.title.clone(), Style::new().bold()),
    ]));
    let status = match (n.kind, n.status) {
        (Kind::Known, _) => "known result",
        (_, Status::Open) => "open: dependencies not asked yet",
        (_, Status::Expanded) => "expanded",
        (_, Status::Basic) => "basic: needs nothing more",
    };
    let mut flags = vec![
        Span::styled(n.id.clone(), Style::new().dim()),
        Span::raw(format!(" · {status}")),
    ];
    if n.main {
        flags.push(Span::styled(
            " · main result",
            Style::new().fg(Color::Yellow),
        ));
    }
    if n.appendix {
        flags.push(Span::raw(" · proof in appendix"));
    }
    lines.push(Line::from(flags));
    match &outline {
        Ok(o) => {
            let s = &o.sections[o.section_of[v]];
            lines.push(Line::styled(
                format!("Section {} · {}", s.number, s.title),
                Style::new().dim(),
            ));
        }
        Err(_) => lines.push(Line::styled(
            "The dependencies have a cycle (see Checks).",
            Style::new().fg(Color::Red),
        )),
    }
    lines.push(Line::raw(""));
    if n.statement.is_empty() {
        lines.push(Line::styled(
            "(no statement: e to edit)",
            Style::new().dim().italic(),
        ));
    } else {
        lines.push(Line::raw(n.statement.clone()));
    }
    if n.kind == Kind::Known {
        let cite = if n.cite.is_empty() {
            "citation missing"
        } else {
            &n.cite
        };
        lines.push(Line::styled(
            format!("Source: {cite}"),
            Style::new().fg(Color::Blue),
        ));
    }

    let reference = |id: &str| -> Line<'static> {
        match p.index(id) {
            Some(u) => {
                let num = outline
                    .as_ref()
                    .map_or(String::new(), |o| format!(" {}", o.numbers[u]));
                Line::from(vec![
                    Span::raw("  "),
                    status_marker(p.nodes[u].kind, p.nodes[u].status),
                    Span::styled(
                        format!(" {}{num} ", p.nodes[u].kind.printed()),
                        Style::new().fg(ACCENT),
                    ),
                    Span::raw(p.nodes[u].title.clone()),
                ])
            }
            None => Line::styled(format!("  ‹{id}› missing"), Style::new().fg(Color::Red)),
        }
    };
    lines.push(Line::raw(""));
    lines.push(Line::styled("Uses", Style::new().bold()));
    if n.uses.is_empty() {
        let hint = match n.status {
            Status::Open if n.kind.expandable() => "  nothing yet: Enter asks what it depends on",
            _ => "  nothing",
        };
        lines.push(Line::styled(hint, Style::new().dim()));
    }
    for u in &n.uses {
        lines.push(reference(u));
    }
    let users = p.users_of(&n.id);
    lines.push(Line::raw(""));
    lines.push(Line::styled("Used by", Style::new().bold()));
    if users.is_empty() {
        lines.push(Line::styled(
            if n.main {
                "  nothing (main result)"
            } else {
                "  nothing"
            },
            Style::new().dim(),
        ));
    }
    for u in users {
        lines.push(reference(&u.id));
    }
    let intro: Vec<_> = p
        .symbols
        .iter()
        .filter(|s| s.introduced_by == n.id)
        .collect();
    if !intro.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Introduces", Style::new().bold()));
        for s in intro {
            lines.push(Line::raw(format!("  {}  {}", s.notation, s.meaning)));
        }
    }
    if !n.symbols.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Mentions", Style::new().bold()));
        lines.push(Line::raw(format!("  {}", n.symbols.join("  "))));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block("Detail", false)),
        right,
    );
}

fn draw_alphabet(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.paper;
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(area);

    let fams: Vec<ListItem> = p
        .families
        .iter()
        .map(|fam| {
            let mut spans = vec![Span::styled(
                format!("{:<10}", fam.name),
                Style::new().fg(ACCENT),
            )];
            for l in &fam.letters {
                let used = p.symbols.iter().any(|s| decorates(&s.notation, l));
                let style = if used {
                    Style::new().fg(Color::Green).bold()
                } else {
                    Style::new().dim()
                };
                spans.push(Span::raw(" "));
                spans.push(Span::styled(l.clone(), style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let focus = app.alpha_focus == AlphaFocus::Families;
    let list = List::new(fams)
        .block(block("Families · reserved letters (green: in use)", focus))
        .highlight_style(if focus {
            Style::new().bg(Color::DarkGray)
        } else {
            Style::new()
        });
    let mut state = ListState::default().with_selected(Some(app.fam_sel));
    f.render_stateful_widget(list, left, &mut state);

    let focus = app.alpha_focus == AlphaFocus::Symbols;
    let items: Vec<ListItem> = p
        .symbols
        .iter()
        .map(|s| {
            let intro = if s.introduced_by.is_empty() {
                "general".to_string()
            } else {
                p.label(&s.introduced_by)
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{:<14}", s.notation), Style::new().bold()),
                    Span::raw(s.meaning.clone()),
                ]),
                Line::styled(
                    format!(
                        "{:<14}{} · {}{}",
                        "",
                        if s.family.is_empty() {
                            "no family"
                        } else {
                            &s.family
                        },
                        intro,
                        if s.macro_name.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", s.macro_name)
                        }
                    ),
                    Style::new().dim(),
                ),
            ])
        })
        .collect();
    if items.is_empty() {
        let msg = Paragraph::new(vec![
            Line::raw(""),
            Line::raw("  Halmos: decide the alphabet before writing. One letter, one meaning;"),
            Line::raw("  related objects get related letters (x ∈ X, f: X → Y)."),
            Line::raw(""),
            Line::raw("  a adds a symbol; it offers the first free letter of its family."),
        ])
        .block(block("Symbols", focus));
        f.render_widget(msg, right);
        return;
    }
    let list = List::new(items)
        .block(block(&format!("Symbols · {}", p.symbols.len()), focus))
        .highlight_style(if focus {
            Style::new().bg(Color::DarkGray)
        } else {
            Style::new()
        });
    let mut state = ListState::default().with_selected(Some(app.sym_sel));
    f.render_stateful_widget(list, right, &mut state);
}

fn draw_outline(f: &mut Frame, area: Rect, app: &App) {
    let md = export::markdown(&app.paper);
    let lines: Vec<Line> = md
        .lines()
        .map(|l| {
            if l.starts_with("# ") {
                Line::styled(l.to_string(), Style::new().fg(ACCENT).bold().underlined())
            } else if l.starts_with("## ") {
                Line::styled(l.to_string(), Style::new().fg(ACCENT).bold())
            } else if l.starts_with("### ") {
                Line::styled(l.to_string(), Style::new().bold())
            } else if l.starts_with("  ") || l.starts_with('>') || l.starts_with('*') {
                Line::styled(l.to_string(), Style::new().dim())
            } else {
                Line::raw(l.to_string())
            }
        })
        .collect();
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0))
        .block(block(
            "Outline · `lemma outline` prints it, `lemma latex` writes the skeleton",
            true,
        ));
    f.render_widget(para, area);
}

fn draw_checks(f: &mut Frame, area: Rect, app: &App) {
    let diags = checks::run(&app.paper);
    let mut lines = vec![];
    if diags.is_empty() {
        lines.push(Line::styled("  All good.", Style::new().fg(Color::Green)));
    }
    for d in &diags {
        let (tag, color) = match d.level {
            Level::Error => ("error  ", Color::Red),
            Level::Warning => ("warning", Color::Yellow),
            Level::Info => ("info   ", Color::Blue),
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {tag} "), Style::new().fg(color).bold()),
            Span::raw(d.message.clone()),
        ]));
    }
    let title = format!(
        "Checks · {} error(s), {} warning(s)",
        diags.iter().filter(|d| d.level == Level::Error).count(),
        diags.iter().filter(|d| d.level == Level::Warning).count()
    );
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .block(block(&title, true)),
        area,
    );
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_modal(f: &mut Frame, modal: &Modal, app: &App) {
    let area = f.area();
    let width = (area.width * 3 / 4).max(60);
    match modal {
        Modal::Form(form) => draw_form(
            f,
            popup(area, width, form.fields.len() as u16 * 2 + 6),
            form,
        ),
        Modal::Picker(p) => draw_picker(f, popup(area, width, 24), p, app),
        Modal::Unlink { target, sel } => {
            let r = popup(area, width, 14);
            f.render_widget(Clear, r);
            let uses = app
                .paper
                .node(target)
                .map(|n| n.uses.clone())
                .unwrap_or_default();
            let items: Vec<ListItem> = uses
                .iter()
                .map(|u| ListItem::new(app.paper.label(u)))
                .collect();
            let list = List::new(items)
                .block(block(
                    &format!(
                        "{} stops using… (Enter removes, Esc closes)",
                        app.paper.label(target)
                    ),
                    true,
                ))
                .highlight_style(Style::new().bg(Color::DarkGray));
            f.render_stateful_widget(list, r, &mut ListState::default().with_selected(Some(*sel)));
        }
        Modal::Confirm { message, .. } => {
            let r = popup(area, width, 5);
            f.render_widget(Clear, r);
            let text = vec![
                Line::raw(message.clone()),
                Line::styled("y / Enter: yes · n / Esc: no", Style::new().dim()),
            ];
            f.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .block(block("Confirm", true)),
                r,
            );
        }
        Modal::Help => {
            let r = popup(area, width, 30);
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(help()).block(block("Help · any key closes", true)),
                r,
            );
        }
    }
}

fn help() -> Vec<Line<'static>> {
    let key = |k: &str, what: &str| {
        Line::from(vec![
            Span::styled(format!("  {k:<14}"), Style::new().fg(ACCENT)),
            Span::raw(what.to_string()),
        ])
    };
    vec![
        Line::styled("Statements", Style::new().bold()),
        key("i", "interview: ask what each open statement depends on"),
        key("Enter", "interview starting at the selected statement"),
        key("a / A", "add a statement / a main result"),
        key("u / x", "add / remove a dependency"),
        key("e / d", "edit / delete"),
        key("m", "toggle main result"),
        key("p", "toggle proof in the appendix"),
        key("b", "toggle basic (nothing more to expand)"),
        key("s", "add a symbol introduced by the selected statement"),
        Line::styled("Interview", Style::new().bold()),
        key("type + Enter", "new statement with that title"),
        key("↓ + Enter", "use an existing statement"),
        key("Enter (empty)", "done with this one, go to the next"),
        key(
            "Ctrl+B / Ctrl+K",
            "basic / known result from the literature",
        ),
        key("Ctrl+N / Esc", "skip / pause"),
        Line::styled("Markers", Style::new().bold()),
        Line::raw("  ★ main   ? open   ✓ expanded   · basic   § known"),
        Line::styled("Everywhere", Style::new().bold()),
        key("1-4 / Tab", "switch tab"),
        key("P", "paper title, story and TCS section"),
        key("q", "quit (everything is saved as you go)"),
    ]
}

fn draw_form(f: &mut Frame, r: Rect, form: &Form) {
    f.render_widget(Clear, r);
    let outer = block(&form.title, true);
    let inner = outer.inner(r);
    f.render_widget(outer, r);
    let label_w = 16u16;
    for (i, field) in form.fields.iter().enumerate() {
        let y = inner.y + 1 + i as u16 * 2;
        if y >= inner.y + inner.height {
            break;
        }
        let focused = i == form.focus;
        let label_style = if focused {
            Style::new().fg(ACCENT).bold()
        } else {
            Style::new().dim()
        };
        f.render_widget(
            Line::styled(
                format!("{:>w$} ", field.label(), w = label_w as usize - 1),
                label_style,
            ),
            Rect {
                x: inner.x,
                y,
                width: label_w,
                height: 1,
            },
        );
        let vr = Rect {
            x: inner.x + label_w + 1,
            y,
            width: inner.width.saturating_sub(label_w + 2),
            height: 1,
        };
        match field {
            Field::Text { input, .. } => {
                let (text, cursor) = input.view(vr.width as usize);
                let style = if focused {
                    Style::new().bg(Color::DarkGray)
                } else {
                    Style::new()
                };
                f.render_widget(Paragraph::new(text).style(style), vr);
                if focused {
                    f.set_cursor_position((vr.x + cursor as u16, vr.y));
                }
            }
            Field::Choice { options, sel, .. } => {
                let text = format!("◀ {} ▶", options.get(*sel).map_or("", |s| s.as_str()));
                let style = if focused {
                    Style::new().bg(Color::DarkGray).bold()
                } else {
                    Style::new()
                };
                f.render_widget(Line::styled(text, style), vr);
            }
        }
    }
    let bottom = inner.y + inner.height.saturating_sub(1);
    let hint = match &form.error {
        Some(e) => Line::styled(e.clone(), Style::new().fg(Color::Red)),
        None => Line::styled(
            "Enter next · ↑↓ move · ←→ choose · Ctrl+S save · Esc cancel",
            Style::new().dim(),
        ),
    };
    f.render_widget(
        hint,
        Rect {
            x: inner.x + 1,
            y: bottom,
            width: inner.width.saturating_sub(1),
            height: 1,
        },
    );
}

fn draw_picker(f: &mut Frame, r: Rect, p: &Picker, app: &App) {
    f.render_widget(Clear, r);
    let paper = &app.paper;
    let open = paper.open_queue().len();
    let title = if p.interview {
        format!("Interview · {open} open")
    } else {
        "Add a dependency".to_string()
    };
    let outer = block(&title, true);
    let inner = outer.inner(r);
    f.render_widget(outer, r);

    let target = paper.node(&p.target);
    let mut head = vec![Line::styled(
        if p.interview {
            format!("What does {} depend on?", paper.label(&p.target))
        } else {
            format!("What else does {} use?", paper.label(&p.target))
        },
        Style::new().bold(),
    )];
    if let Some(n) = target
        && !n.statement.is_empty()
    {
        head.push(Line::styled(
            n.statement.clone(),
            Style::new().italic().dim(),
        ));
    }
    let uses = target.map(|n| n.uses.clone()).unwrap_or_default();
    let uses_text = if uses.is_empty() {
        "nothing yet".to_string()
    } else {
        uses.iter()
            .map(|u| paper.label(u))
            .collect::<Vec<_>>()
            .join(", ")
    };
    head.push(Line::styled(
        format!("Uses so far: {uses_text}"),
        Style::new().dim(),
    ));

    let hints = if p.interview {
        vec![
            Line::styled(
                "type a title, Enter: new statement · ↓ pick one, Enter: use it",
                Style::new().dim(),
            ),
            Line::styled(
                "empty Enter: done, next · ^B basic · ^K known result · ^N skip · Esc pause",
                Style::new().dim(),
            ),
        ]
    } else {
        vec![
            Line::styled(
                "type a title, Enter: new statement · ↓ pick one, Enter: use it",
                Style::new().dim(),
            ),
            Line::styled("Enter on empty or Esc: close", Style::new().dim()),
        ]
    };
    let [head_r, input_r, list_r, hint_r] = Layout::vertical([
        Constraint::Length(head.len() as u16 + 1),
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    f.render_widget(Paragraph::new(head).wrap(Wrap { trim: false }), head_r);

    let prompt = "› ";
    let (text, cursor) = p.input.view(input_r.width.saturating_sub(3) as usize);
    f.render_widget(
        Line::from(vec![
            Span::styled(prompt, Style::new().fg(ACCENT).bold()),
            Span::raw(text),
        ]),
        input_r,
    );
    f.set_cursor_position((input_r.x + 2 + cursor as u16, input_r.y));

    let cands = app.candidates(p);
    let items: Vec<ListItem> = cands
        .iter()
        .map(|&v| {
            let n = &paper.nodes[v];
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<6}", n.kind.short()), Style::new().fg(ACCENT)),
                Span::raw(n.title.clone()),
                Span::styled(format!("  {}", n.id), Style::new().dim()),
            ]))
        })
        .collect();
    let title = if cands.is_empty() {
        if p.input.is_empty() {
            "no existing statements to use"
        } else {
            "no match: Enter creates it"
        }
    } else {
        "existing statements"
    };
    let list = List::new(items)
        .block(Block::new().title(Line::styled(title, Style::new().dim())))
        .highlight_style(Style::new().bg(Color::DarkGray))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, list_r, &mut ListState::default().with_selected(p.sel));
    f.render_widget(Paragraph::new(hints), hint_r);
}
