use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
};

fn main() {
    let rect = Rect { x: 0, y: 0, width: 100, height: 20 };
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(rect);
    println!("width: {}, height: {}", inner.width, inner.height);
}
