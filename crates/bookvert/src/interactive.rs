use std::path::Path;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::state::State;
use crate::styles::STYLES;

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
                    let n = state.picked();

                    if n > 0 {
                        if n < state.catalogs.len() {
                            return ViewEvent::PushView(View::Confirm(ConfirmView::default()));
                        }

                        return ViewEvent::Finish;
                    }
                } else if self.index == 1 {
                    return ViewEvent::PushView(View::Name(NameView::new(state.name.as_deref())));
                } else {
                    let category = self.index.saturating_sub(2);
                    let index = state
                        .catalogs
                        .get(category)
                        .and_then(|c| c.picked)
                        .unwrap_or(0);
                    return ViewEvent::PushView(View::Books(BooksView::new(category, index)));
                }
            }
            Esc | Char('q') => {
                return ViewEvent::PopView;
            }
            Char('x') => {
                return ViewEvent::Finish;
            }
            Backspace | Char('c') if self.index >= 2 => {
                let category = self.index.saturating_sub(2);

                if let Some(c) = state.catalogs.get_mut(category) {
                    c.picked = None;
                }
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State, frame: &mut Frame) {
        let mut selected = None;

        let sub_header = {
            let is_selected = self.index == 0;
            let picked_count = state.picked();
            let total_count = state.catalogs.len();
            let all_picked = picked_count == total_count;

            let marker = STYLES.selected(is_selected);
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

            let marker = STYLES.selected(is_selected);
            let style = STYLES.item_style(is_selected, has_name);

            let name_display = state
                .name
                .as_deref()
                .map(|n| format!("Name: {}", n))
                .unwrap_or_else(|| format!("Name: {}", STYLES.no_name()));

            Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(name_display, style),
            ])
        };

        let mut items = Vec::new();

        for (i, catalog) in state.catalogs.iter().enumerate() {
            let is_selected = i.saturating_add(2) == self.index;
            let is_picked = catalog.picked.is_some();

            if is_selected {
                selected = Some(items.len());
            }

            let marker = STYLES.selected(is_selected);
            let style = STYLES.item_style(is_selected, is_picked);

            let picked_info = if let Some(picked) = catalog.picked {
                if let Some(book) = catalog.books.get(picked) {
                    book.name.clone()
                } else {
                    String::new()
                }
            } else {
                "(not selected)".to_string()
            };

            let mut line = Line::from(vec![Span::styled(
                format!("{marker} {}. {picked_info}", catalog.number),
                style,
            )]);

            if is_picked {
                line.push_span(format!(" {}", STYLES.done()));
            }

            line.push_span(Span::styled(
                format!(
                    " ({} {})",
                    catalog.books.len(),
                    pluralize(catalog.books.len(), "book", "books")
                ),
                STYLES.dim_style(),
            ));

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
}

impl BooksView {
    fn new(category: usize, index: usize) -> Self {
        Self {
            category,
            index,
            list_state: ListState::default(),
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
            Enter | Char('o') => {
                if let Some(c) = state.catalogs.get_mut(self.category) {
                    c.picked = Some(self.index);
                }

                return ViewEvent::PopAndSelectNext;
            }
            _ => {}
        }

        ViewEvent::None
    }

    fn draw(&mut self, state: &State, frame: &mut Frame) {
        let Some(catalog) = state.catalogs.get(self.category) else {
            return;
        };

        let mut items = Vec::new();
        let mut selected = None;

        for (i, book) in catalog.books.iter().enumerate() {
            let is_selected = i == self.index;
            let is_picked = catalog.picked == Some(i);

            if is_selected {
                selected = Some(items.len());
            }

            let marker = STYLES.selected(is_selected);
            let style = STYLES.normal_item_style(is_selected, is_picked);

            let dir = book.dir.parent().unwrap_or(Path::new("."));

            items.push(ListItem::new(Span::styled(
                format!("{marker} {}", book.name),
                style,
            )));

            items.push(ListItem::new(Span::styled(
                format!("    pages: {}", book.pages.len()),
                STYLES.dim_style(),
            )));

            items.push(ListItem::new(Span::styled(
                format!("    bytes: {}", book.bytes()),
                STYLES.dim_style(),
            )));

            items.push(ListItem::new(Span::styled(
                format!("    from {}", dir.display()),
                STYLES.dim_style(),
            )));
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
            Enter | Char('o') => {
                if self.index == 0 {
                    if editing {
                        let trimmed = self.input.value().trim();

                        state.name = if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        };

                        return ViewEvent::PopView;
                    } else {
                        self.editing = true;
                    }
                } else if let Some(name) = state.names.iter().nth(self.index.saturating_sub(1)) {
                    state.name = Some(name.clone());
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

    fn draw(&mut self, state: &State, frame: &mut Frame) {
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
            let is_current = state.name.as_deref() == Some(name.as_str());

            if is_selected {
                selected = Some(items.len());
            }

            let marker = STYLES.selected(is_selected);
            let style = STYLES.normal_item_style(is_selected, is_current);

            items.push(ListItem::new(Span::styled(
                format!("{marker} {name}"),
                style,
            )));
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

    fn draw(&mut self, state: &State, frame: &mut Frame) {
        let picked_count = state.picked();
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

        let prompt = Line::from("Continue anyway? ");

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

/// The interactive application of bookvert.
#[derive(Default)]
pub struct App {
    views: Vec<View>,
}

impl App {
    /// Run the interactive application.
    pub fn run(&mut self, state: &mut State) -> Result<bool> {
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
                        && let Some(category) =
                            state.catalogs.iter().position(|c| c.picked.is_none())
                    {
                        v.index = category.saturating_add(2);
                        self.views.push(View::Books(BooksView::new(category, 0)));
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

fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}
