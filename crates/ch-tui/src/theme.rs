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
    match std::env::var("CROW_THEME").ok().as_deref() {
        Some("high-contrast") | Some("hc") => HIGH_CONTRAST_THEME,
        _ => DEFAULT_THEME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_default() {
        std::env::remove_var("CROW_THEME");
        let t = from_env();
        assert_eq!(t.name, "default");
    }

    #[test]
    fn from_env_hc() {
        std::env::set_var("CROW_THEME", "hc");
        let t = from_env();
        assert_eq!(t.name, "high-contrast");
    }

    #[test]
    fn from_env_high_contrast() {
        std::env::set_var("CROW_THEME", "high-contrast");
        let t = from_env();
        assert_eq!(t.name, "high-contrast");
    }
}
