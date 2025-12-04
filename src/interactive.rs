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
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::State;

/// Centralized styling configuration for the TUI.
struct Styles {
    selected_marker: &'static str,
    done_marker: &'static str,
    empty_marker: &'static str,
    editing_marker: &'static str,
    color_done: Color,
    color_normal: Color,
    color_not_done: Color,
    color_dim: Color,
    color_header: Color,
    color_editing: Color,
    color_warning: Color,
}

impl Styles {
    fn marker(&self, selected: bool, done: bool) -> &'static str {
        if selected {
            self.selected_marker
        } else if done {
            self.done_marker
        } else {
            self.empty_marker
        }
    }

    fn item_style(&self, selected: bool, done: bool) -> Style {
        let mut s = Style::default();

        if done {
            s = s.fg(self.color_done);
        } else {
            s = s.fg(self.color_not_done);
        };

        if selected {
            s = s.add_modifier(Modifier::BOLD);
        }

        s
    }

    fn normal_item_style(&self, selected: bool, done: bool) -> Style {
        let mut s = Style::default();

        if done {
            s = s.fg(self.color_done);
        } else {
            s = s.fg(self.color_normal);
        };

        if selected {
            s = s.add_modifier(Modifier::BOLD);
        }

        s
    }

    fn header_style(&self) -> Style {
        Style::default().fg(self.color_header).bold()
    }

    fn header_hint_style(&self) -> Style {
        Style::default().fg(self.color_header)
    }

    fn dim_style(&self) -> Style {
        Style::default().fg(self.color_dim)
    }

    fn warning_style(&self) -> Style {
        Style::default().fg(self.color_warning).bold()
    }

    fn warning_text_style(&self) -> Style {
        Style::default().fg(self.color_warning)
    }

    fn button_style(&self, selected: bool, positive: bool) -> Style {
        let mut s = Style::default();

        if positive {
            s = s.fg(self.color_done);
        } else {
            s = s.fg(self.color_not_done);
        }

        if selected {
            s = s.add_modifier(Modifier::BOLD);
        }

        s
    }

    fn input_style(&self, selected: bool, editing: bool) -> Style {
        let mut s = Style::default();

        if editing {
            s = s.fg(self.color_editing).add_modifier(Modifier::BOLD);
        } else if selected {
            s = s.fg(self.color_normal).add_modifier(Modifier::BOLD);
        } else {
            s = s.fg(self.color_normal);
        }

        s
    }

    fn input_marker(&self, selected: bool, editing: bool) -> &'static str {
        if editing {
            self.editing_marker
        } else if selected {
            self.selected_marker
        } else {
            self.empty_marker
        }
    }
}

/// Global styles instance.
const STYLES: Styles = Styles {
    selected_marker: "*",
    done_marker: "✓",
    empty_marker: " ",
    editing_marker: ">",
    color_done: Color::Green,
    color_normal: Color::Reset,
    color_not_done: Color::Red,
    color_dim: Color::DarkGray,
    color_header: Color::Cyan,
    color_editing: Color::Cyan,
    color_warning: Color::Yellow,
};

enum ViewEvent {
    PushView(View),
    PopView,
    PopAndSelectNext,
    Finish,
    None,
}

struct CatalogsView {
    index: usize,
    list_state: ListState,
}

impl Default for CatalogsView {
    fn default() -> Self {
        Self {
            index: 1,
            list_state: ListState::default(),
        }
    }
}

impl CatalogsView {
    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Backspace, Char, Down, Enter, Esc, Right, Up};

        let max_index = state.catalogs.len().saturating_add(1);

        match key.code {
            Up | Char('k') => {
                self.index = self.index.saturating_sub(1);
            }
            Down | Char('j') => {
                self.index = self.index.saturating_add(1).min(max_index);
            }
            Right | Enter | Char('l' | 'o' | ' ') => {
                if self.index == 0 {
                    if !state.picked.is_empty() {
                        if state.picked.len() < state.catalogs.len() {
                            return ViewEvent::PushView(View::Confirm(ConfirmView::default()));
                        }
                        return ViewEvent::Finish;
                    }
                } else if self.index == 1 {
                    return ViewEvent::PushView(View::Name(NameView::new(state.name.as_deref())));
                } else {
                    let category = self.index.saturating_sub(2);
                    let index = state.picked.get(&category).copied().unwrap_or(0);
                    return ViewEvent::PushView(View::Books(BooksView::new(category, index)));
                }
            }
            Esc | Char('q') => {
                return ViewEvent::PopView;
            }
            Char('x') if !state.picked.is_empty() => {
                return ViewEvent::Finish;
            }
            Backspace | Char('c') if self.index >= 2 => {
                let category = self.index.saturating_sub(2);
                state.picked.remove(&category);
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let mut selected = None;

        let sub_header = {
            let is_selected = self.index == 0;
            let picked_count = state.picked.len();
            let total_count = state.catalogs.len();
            let all_picked = picked_count == total_count;

            let marker = STYLES.marker(is_selected, all_picked);
            let style = STYLES.normal_item_style(is_selected, all_picked);

            Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(
                    format!("Run bookvert with {picked_count}/{total_count} selected"),
                    style,
                ),
            ])
        };

        let name_line = {
            let is_selected = self.index == 1;
            let has_name = state.name.is_some();

            let marker = STYLES.marker(is_selected, has_name);
            let style = STYLES.item_style(is_selected, has_name);

            let name_display = state
                .name
                .as_deref()
                .map(|n| format!("Name: {}", n))
                .unwrap_or_else(|| "Name: (not set)".to_string());
            Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(name_display, style),
            ])
        };

        let mut items = Vec::new();

        for (i, catalog) in state.catalogs.iter().enumerate() {
            let is_selected = i.saturating_add(2) == self.index;
            let is_picked = state.picked.contains_key(&i);

            if is_selected {
                selected = Some(items.len());
            }

            let marker = STYLES.marker(is_selected, is_picked);
            let style = STYLES.item_style(is_selected, is_picked);

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
                Span::styled(format!("{marker} "), style),
                Span::styled(format!("{:03}", catalog.number), style),
                Span::styled(picked_info, style),
                Span::styled(
                    format!(" ({} options)", catalog.books.len()),
                    STYLES.dim_style(),
                ),
            ]);

            items.push(ListItem::new(line));

            if is_selected {
                selected = Some(items.len().saturating_sub(1));
            }
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let header = Line::from(vec![
            Span::styled("Catalogs", STYLES.header_style()),
            Span::styled(
                " (Enter/o/→ to select, Delete/c to clear, Esc/q to quit)",
                STYLES.header_hint_style(),
            ),
        ]);

        let list = List::new(items);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);

        let separator = Line::from(Span::styled(
            "─".repeat(frame.area().width as usize),
            STYLES.dim_style(),
        ));

        let area = frame.area();

        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

        frame.render_widget(Paragraph::new(header), layout[0]);
        frame.render_widget(Paragraph::new(sub_header), layout[1]);
        frame.render_widget(name_line, layout[2]);
        frame.render_widget(separator, layout[3]);
        frame.render_stateful_widget(list, layout[4], &mut self.list_state);
        frame.render_stateful_widget(scrollbar, layout[4], &mut scrollbar_state);
    }
}

struct BooksView {
    category: usize,
    index: usize,
    list_state: ListState,
    expanded: HashSet<usize>,
}

impl BooksView {
    fn new(category: usize, index: usize) -> Self {
        Self {
            category,
            index,
            list_state: ListState::default(),
            expanded: HashSet::new(),
        }
    }

    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Char, Down, Enter, Esc, Left, Up};

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
                return ViewEvent::PopAndSelectNext;
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

            let marker = STYLES.marker(is_selected, is_picked);
            let style = STYLES.normal_item_style(is_selected, is_picked);

            let line = Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(
                    format!(
                        "{} ({} pages, {} bytes)",
                        book.name,
                        book.pages.len(),
                        book.bytes(),
                    ),
                    style,
                ),
            ]);

            items.push(ListItem::new(line));

            if self.expanded.contains(&i) {
                let path_line = Line::from(Span::styled(
                    format!("    {}", book.dir.display()),
                    STYLES.dim_style(),
                ));
                items.push(ListItem::new(path_line));
            }
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let line = format!("Catalog {:03} - Select book", catalog.number);
        let line = Line::from(vec![
            Span::styled(line, STYLES.header_style()),
            Span::styled(
                " (Enter/o to pick, Esc/q/← to go back, i/I to show paths)",
                STYLES.header_hint_style(),
            ),
        ]);

        let list = List::new(items);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);

        let area = frame.area();
        let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        frame.render_widget(line, layout[0]);
        frame.render_stateful_widget(list, layout[1], &mut self.list_state);
        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}

struct NameView {
    index: usize,
    input: Input,
    editing: bool,
    list_state: ListState,
}

impl NameView {
    fn new(current_name: Option<&str>) -> Self {
        Self {
            index: 0,
            input: Input::new(current_name.unwrap_or_default().to_string()),
            editing: false,
            list_state: ListState::default(),
        }
    }

    fn update(&mut self, key: KeyEvent, state: &mut State) -> ViewEvent {
        use KeyCode::{Char, Down, Enter, Esc, Left, Up};

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
                        let trimmed = self.input.value().trim();

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
            _ if editing => {
                self.input.handle_event(&Event::Key(key));
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let editing = self.editing && self.index == 0;

        let header = Line::from(vec![
            Span::styled("Set Name", STYLES.header_style()),
            Span::styled(
                " (Enter to select, Esc/q/← to go back)",
                STYLES.header_hint_style(),
            ),
        ]);

        let is_custom_selected = self.index == 0;
        let input_marker = STYLES.input_marker(is_custom_selected, editing);
        let input_style = STYLES.input_style(is_custom_selected, editing);

        let input_text = if self.input.value().is_empty() && !editing {
            "(enter custom name)".to_string()
        } else {
            self.input.value().to_string()
        };

        let input_line = Line::from(vec![
            Span::styled(format!("{input_marker} "), input_style),
            Span::styled(&input_text, input_style),
        ]);

        let separator = Line::from(Span::styled(
            "─".repeat(frame.area().width as usize),
            STYLES.dim_style(),
        ));

        let mut items = Vec::new();
        let mut selected = None;

        for (i, name) in state.names.iter().enumerate() {
            let is_selected = i.saturating_add(1) == self.index;
            let is_current = state.name.as_deref() == Some(*name);

            if is_selected {
                selected = Some(items.len());
            }

            let marker = STYLES.marker(is_selected, is_current);
            let style = STYLES.normal_item_style(is_selected, is_current);

            let line = Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(name.to_string(), style),
            ]);

            items.push(ListItem::new(line));
        }

        self.list_state.select(selected);

        let mut scrollbar_state = ScrollbarState::new(items.len())
            .position(self.list_state.selected().unwrap_or_default());

        let area = frame.area();
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

        frame.render_widget(header, layout[0]);
        frame.render_widget(Paragraph::new(input_line), layout[1]);
        frame.render_widget(separator, layout[2]);

        let list = List::new(items);
        frame.render_stateful_widget(list, layout[3], &mut self.list_state);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, layout[3], &mut scrollbar_state);

        if editing {
            let cursor_x = layout[1].x + 2 + self.input.visual_cursor() as u16;
            let cursor_y = layout[1].y;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

#[derive(Default)]
struct ConfirmView {
    selected: bool,
}

impl ConfirmView {
    fn update(&mut self, key: KeyEvent, _state: &mut State) -> ViewEvent {
        use KeyCode::{Char, Enter, Esc, Left, Right};

        match key.code {
            Left | Char('h') => {
                self.selected = false;
            }
            Right | Char('l') => {
                self.selected = true;
            }
            Enter | Char(' ') | Char('o') => {
                if self.selected {
                    return ViewEvent::Finish;
                } else {
                    return ViewEvent::PopView;
                }
            }
            Esc | Char('q' | 'n') => {
                return ViewEvent::PopView;
            }
            Char('y') => {
                return ViewEvent::Finish;
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State<'_, '_>, frame: &mut Frame) {
        let picked_count = state.picked.len();
        let total_count = state.catalogs.len();
        let missing = total_count.saturating_sub(picked_count);

        let area = frame.area();
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

        let header = Line::from(vec![Span::styled("⚠ Warning", STYLES.warning_style())]);

        let message = Line::from(vec![Span::styled(
            format!("Selection incomplete: {missing} catalog(s) not selected."),
            STYLES.warning_text_style(),
        )]);

        let prompt = Line::from(vec![Span::styled("Continue anyway? ", Style::default())]);

        let no_style = STYLES.button_style(!self.selected, false);
        let yes_style = STYLES.button_style(self.selected, true);

        let buttons = Line::from(vec![
            Span::styled("[No/n]", no_style),
            Span::raw("  "),
            Span::styled("[Yes/y]", yes_style),
        ]);

        frame.render_widget(header, layout[0]);
        frame.render_widget(message, layout[1]);
        frame.render_widget(prompt, layout[2]);
        frame.render_widget(buttons, layout[3]);
    }
}

enum View {
    Catalogs(CatalogsView),
    Books(BooksView),
    Name(NameView),
    Confirm(ConfirmView),
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
                View::Confirm(v) => v.draw(state, frame),
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
                View::Confirm(v) => v.update(key, state),
            };

            match ev {
                ViewEvent::PushView(view) => {
                    self.views.push(view);
                }
                ViewEvent::PopView => {
                    self.views.pop();
                }
                ViewEvent::PopAndSelectNext => {
                    self.views.pop();
                    if let Some(View::Catalogs(v)) = self.views.last_mut()
                        && let Some(next) =
                            (0..state.catalogs.len()).find(|i| !state.picked.contains_key(i))
                    {
                        v.index = next.saturating_add(2);
                    }
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
