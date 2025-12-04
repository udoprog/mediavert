use std::collections::HashSet;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::State;

enum ViewEvent {
    PushView(View),
    PopView,
    Finish,
    None,
}

#[derive(Default)]
struct CatalogsView {
    index: usize,
    scroll_x: u16,
    list_state: ListState,
}

impl CatalogsView {
    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Char, Down, Enter, Esc, Left, Right, Up};

        match key.code {
            Up | Char('k') => {
                self.index = self.index.saturating_sub(1);
            }
            Down | Char('j') => {
                self.index = self.index.saturating_add(1).min(state.catalogs.len());
            }
            Left | Char('h') => {
                self.scroll_x = self.scroll_x.saturating_sub(4);
            }
            Right | Enter | Char('l' | 'o' | ' ') => {
                let view = if self.index == 0 {
                    View::Name(NameView::new(state.name.as_deref()))
                } else {
                    let category = self.index.saturating_sub(1);
                    let index = state.picked.get(&category).copied().unwrap_or(0);
                    View::Books(BooksView::new(category, index))
                };

                return ViewEvent::PushView(view);
            }
            Esc | Char('q') => {
                return ViewEvent::PopView;
            }
            Char('x') if !state.picked.is_empty() => {
                return ViewEvent::Finish;
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let mut items = Vec::new();
        let mut selected = None;

        let is_name_selected = self.index == 0;
        let name_color = if state.name.is_some() {
            Color::Green
        } else {
            Color::Red
        };
        let (prefix, style) = if is_name_selected {
            selected = Some(0);
            (
                "* ",
                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::default().fg(name_color))
        };
        let name_display = state
            .name
            .as_deref()
            .map(|n| format!("Name: {}", n))
            .unwrap_or_else(|| "Name: (not set)".to_string());
        let name_line = Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(name_display, style),
        ]);
        items.push(ListItem::new(name_line));

        for (i, catalog) in state.catalogs.iter().enumerate() {
            let is_selected = i.saturating_add(1) == self.index;
            let is_picked = state.picked.contains_key(&i);

            if is_selected {
                selected = Some(items.len());
            }

            let base_color = if is_picked { Color::Green } else { Color::Red };

            let (prefix, style) = if is_selected {
                (
                    "* ",
                    Style::default().fg(base_color).add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(base_color))
            };

            let picked_info = if let Some(&book_idx) = state.picked.get(&i) {
                if let Some(book) = catalog.books.get(book_idx) {
                    format!(" {}", book.name)
                } else {
                    String::new()
                }
            } else {
                " (not selected)".to_string()
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:03}", catalog.number), style),
                Span::styled(picked_info, style),
                Span::styled(
                    format!(" ({} options)", catalog.books.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            items.push(ListItem::new(line));
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        let line = Line::from(vec![
            Span::styled("Catalogs", Style::default().fg(Color::Cyan).bold()),
            Span::styled(
                " (Enter/o/→ to select, Esc/q to quit)",
                Style::default().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(line).scroll((0, self.scroll_x)), layout[0]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);

        let picked_count = state.picked.len();
        let total_count = state.catalogs.len();
        let execute_style = if picked_count > 0 {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let footer = Line::from(vec![Span::styled(
            format!("[x] Execute with {picked_count}/{total_count} selected"),
            execute_style,
        )]);
        frame.render_widget(Paragraph::new(footer), layout[2]);
    }
}

struct BooksView {
    category: usize,
    index: usize,
    scroll_x: u16,
    list_state: ListState,
    expanded: HashSet<usize>,
}

impl BooksView {
    fn new(category: usize, index: usize) -> Self {
        Self {
            category,
            index,
            scroll_x: 0,
            list_state: ListState::default(),
            expanded: HashSet::new(),
        }
    }

    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Char, Down, Enter, Esc, Left, Right, Up};

        match key.code {
            Up | Char('k') => {
                self.index = self.index.saturating_sub(1);
            }
            Down | Char('j') => {
                if let Some(catalog) = state.catalogs.get(self.category) {
                    self.index = self
                        .index
                        .saturating_add(1)
                        .min(catalog.books.len().saturating_sub(1));
                }
            }
            Left | Char('h') | Esc | Char('q') => {
                return ViewEvent::PopView;
            }
            Right | Char('l') => {
                self.scroll_x = self.scroll_x.saturating_add(4);
            }
            Char('I') => {
                if let Some(catalog) = state.catalogs.get(self.category) {
                    if self.expanded.len() == catalog.books.len() {
                        self.expanded.clear();
                    } else {
                        self.expanded.extend(0..catalog.books.len());
                    }
                }
            }
            Char('i' | ' ') => {
                if self.expanded.contains(&self.index) {
                    self.expanded.remove(&self.index);
                } else {
                    self.expanded.insert(self.index);
                }
            }
            Enter | Char('o') => {
                state.picked.insert(self.category, self.index);
                return ViewEvent::PopView;
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let Some(catalog) = state.catalogs.get(self.category) else {
            return;
        };

        let mut items = Vec::new();
        let mut selected = None;
        let current_pick = state.picked.get(&self.category).copied();

        for (i, book) in catalog.books.iter().enumerate() {
            let is_selected = i == self.index;
            let is_picked = current_pick == Some(i);

            if is_selected {
                selected = Some(items.len());
            }

            let (prefix, style) = if is_selected {
                (
                    "* ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else if is_picked {
                ("  ", Style::default().fg(Color::Green))
            } else {
                ("  ", Style::default())
            };

            let picked_marker = if is_picked { " ✓" } else { "" };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    format!(
                        "{} ({} pages, {} bytes){}",
                        book.name,
                        book.pages.len(),
                        book.bytes(),
                        picked_marker,
                    ),
                    style,
                ),
            ]);

            items.push(ListItem::new(line));

            if self.expanded.contains(&i) {
                let path_line = Line::from(Span::styled(
                    format!("    {}", book.dir.display()),
                    Style::default().fg(Color::DarkGray),
                ));
                items.push(ListItem::new(path_line));
            }
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        let line = format!("Catalog {:03} - Select book", catalog.number);
        let line = Line::from(vec![
            Span::styled(line, Style::default().fg(Color::Cyan).bold()),
            Span::styled(
                " (Enter/o to pick, Esc/q/← to go back, i/I to show paths)",
                Style::default().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(line).scroll((0, self.scroll_x)), layout[0]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}

struct NameView {
    index: usize,
    input: String,
    editing: bool,
    scroll_x: u16,
    list_state: ListState,
}

impl NameView {
    fn new(current_name: Option<&str>) -> Self {
        Self {
            index: 0,
            input: current_name.unwrap_or_default().to_string(),
            editing: false,
            scroll_x: 0,
            list_state: ListState::default(),
        }
    }

    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Backspace, Char, Down, Enter, Esc, Left, Up};

        let editing = self.editing && self.index == 0;

        match key.code {
            Up if !editing => {
                self.index = self.index.saturating_sub(1);
            }
            Char('k') if !editing => {
                self.index = self.index.saturating_sub(1);
            }
            Down if !editing => {
                self.index = self.index.saturating_add(1).min(state.names.len());
            }
            Char('j') if !editing => {
                self.index = self.index.saturating_add(1).min(state.names.len());
            }
            Left if !editing => {
                return ViewEvent::PopView;
            }
            Char('h') if !editing => {
                return ViewEvent::PopView;
            }
            Esc => {
                if !editing {
                    return ViewEvent::PopView;
                }

                self.editing = false;
            }
            Char('q') if !editing => {
                return ViewEvent::PopView;
            }
            Enter => {
                if self.index == 0 {
                    if editing {
                        let trimmed = self.input.trim();

                        state.name = if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        };

                        self.editing = false;
                    } else {
                        self.editing = true;
                    }
                } else if let Some(&name) = state.names.get(self.index.saturating_sub(1)) {
                    state.name = Some(name.to_string());
                    return ViewEvent::PopView;
                }
            }
            Backspace if editing => {
                self.input.pop();
            }
            Char(c) if editing => {
                self.input.push(c);
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let mut items = Vec::new();
        let mut selected = None;
        let editing = self.editing && self.index == 0;

        let is_custom_selected = self.index == 0;
        if is_custom_selected {
            selected = Some(0);
        }

        let (prefix, style) = if editing {
            (
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else if is_custom_selected {
            (
                "* ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::default().fg(Color::DarkGray))
        };

        let input_display = if editing {
            format!("{}_", self.input)
        } else if self.input.is_empty() {
            "(enter custom name)".to_string()
        } else {
            format!("Custom: {}", self.input)
        };

        let line = Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(input_display, style),
        ]);
        items.push(ListItem::new(line));

        for (i, name) in state.names.iter().enumerate() {
            let is_selected = i.saturating_add(1) == self.index;
            let is_current = state.name.as_deref() == Some(*name);

            if is_selected {
                selected = Some(items.len());
            }

            let (prefix, style) = if is_selected {
                (
                    "* ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else if is_current {
                ("  ", Style::default().fg(Color::Green))
            } else {
                ("  ", Style::default())
            };

            let current_marker = if is_current { " ✓" } else { "" };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{}{}", name, current_marker), style),
            ]);

            items.push(ListItem::new(line));
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        let line = Line::from(vec![
            Span::styled("Set Name", Style::default().fg(Color::Cyan).bold()),
            Span::styled(
                " (Enter to select, Esc/q/← to go back)",
                Style::default().fg(Color::Cyan),
            ),
        ]);
        frame.render_widget(Paragraph::new(line).scroll((0, self.scroll_x)), layout[0]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}

enum View {
    Catalogs(CatalogsView),
    Books(BooksView),
    Name(NameView),
}

#[derive(Default)]
pub(crate) struct App {
    views: Vec<View>,
}

impl App {
    pub(crate) fn run(&mut self, state: &mut State<'_, '_>) -> Result<bool> {
        self.views.clear();
        self.views.push(View::Catalogs(CatalogsView::default()));

        let mut terminal = ratatui::init();

        let outcome = loop {
            let Some(view) = self.views.last_mut() else {
                break false;
            };

            terminal.draw(|frame| match view {
                View::Catalogs(v) => v.draw(state, frame),
                View::Books(v) => v.draw(state, frame),
                View::Name(v) => v.draw(state, frame),
            })?;

            let e = event::read()?;

            let Event::Key(key) = e else {
                continue;
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            let ev = match view {
                View::Catalogs(v) => v.update(key, state),
                View::Books(v) => v.update(key, state),
                View::Name(v) => v.update(key, state),
            };

            match ev {
                ViewEvent::PushView(view) => {
                    self.views.push(view);
                }
                ViewEvent::PopView => {
                    self.views.pop();
                }
                ViewEvent::Finish => {
                    break true;
                }
                ViewEvent::None => {}
            }
        };

        ratatui::restore();
        Ok(outcome)
    }
}
