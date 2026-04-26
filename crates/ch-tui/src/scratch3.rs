use ratatui::widgets::{Paragraph, Wrap};
use ratatui::text::{Line, Text};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;

fn main() -> io::Result<()> {
    // We just want to check if it compiles and see if there is any documentation
    let mut text = Text::from("Line 1\nLine 2\nLine 3");
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .scroll((100, 0)); // scroll past the end
    Ok(())
}
