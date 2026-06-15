//! Theme system for the TUI.
//!
//! Extracts hardcoded colors into a `Theme` struct.  Switch via
//! `CROW_THEME=hc` env var.  Slash-commanded `/theme` is a future task.

use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub surface: Color,
    pub tab_active_text: Color,
    pub tab_inactive: Color,
    pub border_focused: Color,
    pub agent_cursor: Color,
    pub agent_multi: Color,
    pub agent_meta: Color,
    pub status_idle: Color,
    pub status_thinking: Color,
    pub accent_primary: Color,
    pub overlay_bg: Color,
    pub status_errored: Color,
    pub status_unknown: Color,
    pub suffix: Color,
    pub summary: Color,
    pub footer: Color,
}

pub const DEFAULT_THEME: Theme = Theme {
    name: "default",
    surface: Color::Black,
    tab_active_text: Color::Black,
    tab_inactive: Color::Gray,
    border_focused: Color::LightBlue,
    agent_cursor: Color::Cyan,
    agent_multi: Color::Yellow,
    agent_meta: Color::DarkGray,
    accent_primary: Color::LightBlue,
    overlay_bg: Color::Black,
    status_idle: Color::Green,
    status_thinking: Color::Yellow,
    status_errored: Color::Red,
    status_unknown: Color::DarkGray,
    suffix: Color::DarkGray,
    summary: Color::Gray,
    footer: Color::DarkGray,
};

pub const HIGH_CONTRAST_THEME: Theme = Theme {
    name: "high-contrast",
    surface: Color::Black,
    tab_active_text: Color::Black,
    tab_inactive: Color::Gray,
    border_focused: Color::White,
    agent_cursor: Color::White,
    accent_primary: Color::White,
    overlay_bg: Color::Black,
    agent_multi: Color::LightYellow,
    agent_meta: Color::Gray,
    status_idle: Color::LightGreen,
    status_thinking: Color::LightYellow,
    status_errored: Color::LightRed,
    status_unknown: Color::Gray,
    suffix: Color::Gray,
    summary: Color::White,
    footer: Color::Gray,
};

pub fn from_env() -> Theme {
    from_name(std::env::var("CROW_THEME").ok().as_deref())
}

/// Pure theme selection from an optional `CROW_THEME` value.
///
/// Kept separate from [`from_env`] so the selection logic can be tested
/// without mutating the process-global environment, which races under
/// parallel test execution.
pub fn from_name(value: Option<&str>) -> Theme {
    match value {
        Some("high-contrast") | Some("hc") => HIGH_CONTRAST_THEME,
        _ => DEFAULT_THEME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_name_default() {
        assert_eq!(from_name(None).name, "default");
        assert_eq!(from_name(Some("unrecognized")).name, "default");
    }

    #[test]
    fn from_name_hc() {
        assert_eq!(from_name(Some("hc")).name, "high-contrast");
    }

    #[test]
    fn from_name_high_contrast() {
        assert_eq!(from_name(Some("high-contrast")).name, "high-contrast");
    }
}
