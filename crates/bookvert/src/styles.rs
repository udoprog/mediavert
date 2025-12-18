use ratatui::style::{Color, Modifier, Style, Stylize};

/// Centralized styling configuration for the TUI.
pub(crate) struct Styles {
    selected_marker: &'static str,
    done_marker: &'static str,
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
    pub(crate) fn selected(&self, selected: bool) -> &'static str {
        if selected { self.selected_marker } else { " " }
    }

    pub(crate) fn done(&self) -> &'static str {
        self.done_marker
    }

    pub(crate) fn no_name(&self) -> &'static str {
        "(not set)"
    }

    pub(crate) fn item_style(&self, selected: bool, done: bool) -> Style {
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

    pub(crate) fn normal_item_style(&self, selected: bool, done: bool) -> Style {
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

    pub(crate) fn header_style(&self) -> Style {
        Style::default().fg(self.color_header).bold()
    }

    pub(crate) fn header_hint_style(&self) -> Style {
        Style::default().fg(self.color_header)
    }

    pub(crate) fn dim_style(&self) -> Style {
        Style::default().fg(self.color_dim)
    }

    pub(crate) fn warning_style(&self) -> Style {
        Style::default().fg(self.color_warning).bold()
    }

    pub(crate) fn warning_text_style(&self) -> Style {
        Style::default().fg(self.color_warning)
    }

    pub(crate) fn button_style(&self, selected: bool, positive: bool) -> Style {
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

    pub(crate) fn input_style(&self, selected: bool, editing: bool) -> Style {
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

    pub(crate) fn input_marker(&self, selected: bool, editing: bool) -> &'static str {
        if editing {
            self.editing_marker
        } else if selected {
            self.selected_marker
        } else {
            " "
        }
    }
}

/// Global styles instance.
pub(crate) const STYLES: Styles = Styles {
    selected_marker: "*",
    done_marker: "✓",
    editing_marker: ">",
    color_done: Color::Green,
    color_normal: Color::Reset,
    color_not_done: Color::Red,
    color_dim: Color::DarkGray,
    color_header: Color::Cyan,
    color_editing: Color::Cyan,
    color_warning: Color::Yellow,
};
