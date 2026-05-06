use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use ch_agent::{AgentActivity, AgentRuntime, AgentInfo};
use ch_core::MessageBus;
use ch_protocol::{AgentAddress, AgentId, AgentMessage, MessageType, Payload};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        EnableMouseCapture, DisableMouseCapture, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::{io, time::Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum FocusedPanel {
    Agents,
    Chat,
    Input,
}

/// App state
pub struct App {
    pub runtime: Arc<AgentRuntime>,
    pub bus: Arc<MessageBus>,
    pub user_agent_id: AgentId,
    pub agents: Vec<AgentInfo>,
    pub input: String,
    pub messages: Vec<String>,
    pub selected_agent: usize,
    pub should_quit: bool,
    pub response_rx: mpsc::Receiver<(String, String)>,
    pub tx: mpsc::Sender<(String, String)>,
    pub focused_panel: FocusedPanel,
    pub chat_scroll_offset: usize,
    pub input_scroll_offset: usize,
}

impl App {
    pub fn new(
        runtime: Arc<AgentRuntime>,
        bus: Arc<MessageBus>,
        user_agent_id: AgentId,
        tx: mpsc::Sender<(String, String)>,
        response_rx: mpsc::Receiver<(String, String)>,
    ) -> Self {
        let agents = runtime.list_agents();

        Self {
            runtime,
            bus,
            user_agent_id,
            agents,
            input: String::new(),
            messages: vec!["Welcome to Crow Hub! Type to send a message.".to_string()],
            selected_agent: 0,
            should_quit: false,
            response_rx,
            tx,
            focused_panel: FocusedPanel::Input,
            chat_scroll_offset: 0,
            input_scroll_offset: 0,
        }
    }

    pub fn on_tick(&mut self) {
        while let Ok((agent, response)) = self.response_rx.try_recv() {
            // Streaming intelligence: append to last message if it's from the same agent
            if let Some(last_msg) = self.messages.last_mut() {
                let prefix = format!("{}: ", agent);
                if last_msg.starts_with(&prefix) {
                    last_msg.push_str(&response);
                    continue;
                }
            }
            // Otherwise push a new message
            self.messages.push(format!("{}: {}", agent, response));
        }
    }
}

pub fn run_tui_app(
    runtime: Arc<AgentRuntime>,
    bus: Arc<MessageBus>,
    user_agent_id: AgentId,
    tx: mpsc::Sender<(String, String)>,
    response_rx: mpsc::Receiver<(String, String)>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // EnableBracketedPaste tells the terminal to wrap pastes in ESC[200~ .. ESC[201~
    // and crossterm delivers them as a single Event::Paste(String) event instead of
    // a flood of KeyCode::Char('[') events.
    //
    // NOTE: We deliberately do NOT enable mouse capture. The TUI has no mouse
    // interactions, and some terminals (notably the Antigravity-integrated
    // terminal) leak the mouse-tracking escape sequences (ESC[M…, ESC[<…M)
    // into the input as literal `[` characters when the mouse moves.
    // Update: User specifically requested mouse scrolling, so we will enable it.
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new(runtime, bus, user_agent_id, tx, response_rx);
    let res = run_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = std::time::Instant::now();
    let mut last_key_time = std::time::Instant::now();

    loop {
        terminal.draw(|f| ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            match event::read()? {
                Event::Paste(content) => {
                    // Bracketed paste: append the whole string at once (no
                    // per-character flood of KeyCode::Char events).  Strip
                    // newlines so a multi-line paste doesn't accidentally
                    // submit while typing.
                    let single_line: String = content.replace(['\n', '\r'], " ");
                    app.input.push_str(&single_line);
                }
                Event::Mouse(mouse_event) => {
                    match mouse_event.kind {
                        MouseEventKind::ScrollUp => match app.focused_panel {
                            FocusedPanel::Input => app.input_scroll_offset = app.input_scroll_offset.saturating_sub(1),
                            _ => app.chat_scroll_offset = app.chat_scroll_offset.saturating_add(1),
                        },
                        MouseEventKind::ScrollDown => match app.focused_panel {
                            FocusedPanel::Input => app.input_scroll_offset = app.input_scroll_offset.saturating_add(1),
                            _ => app.chat_scroll_offset = app.chat_scroll_offset.saturating_sub(1),
                        },
                        _ => {}
                    }
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let now = std::time::Instant::now();
                    let is_fast = now.duration_since(last_key_time) < Duration::from_millis(20);
                    last_key_time = now;

                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            app.should_quit = true;
                        }
                        KeyCode::Esc => app.should_quit = true,
                        KeyCode::Tab => {
                            app.focused_panel = match app.focused_panel {
                                FocusedPanel::Input => FocusedPanel::Agents,
                                FocusedPanel::Agents => FocusedPanel::Chat,
                                FocusedPanel::Chat => FocusedPanel::Input,
                            };
                        }
                        KeyCode::BackTab => {
                            app.focused_panel = match app.focused_panel {
                                FocusedPanel::Input => FocusedPanel::Chat,
                                FocusedPanel::Chat => FocusedPanel::Agents,
                                FocusedPanel::Agents => FocusedPanel::Input,
                            };
                        }
                        KeyCode::Up => {
                            match app.focused_panel {
                                FocusedPanel::Agents => {
                                    if app.selected_agent > 0 {
                                        app.selected_agent -= 1;
                                    }
                                }
                                FocusedPanel::Chat => {
                                    app.chat_scroll_offset = app.chat_scroll_offset.saturating_add(1);
                                }
                                FocusedPanel::Input => {
                                    app.input_scroll_offset = app.input_scroll_offset.saturating_sub(1);
                                }
                            }
                        }
                        KeyCode::Down => {
                            match app.focused_panel {
                                FocusedPanel::Agents => {
                                    if app.selected_agent + 1 < app.agents.len() {
                                        app.selected_agent += 1;
                                    }
                                }
                                FocusedPanel::Chat => {
                                    app.chat_scroll_offset = app.chat_scroll_offset.saturating_sub(1);
                                }
                                FocusedPanel::Input => {
                                    app.input_scroll_offset = app.input_scroll_offset.saturating_add(1);
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Enter => {
                            if is_fast {
                                // Un-bracketed paste detection: If Enter arrives too quickly
                                // after another character, treat it as a pasted newline (space).
                                app.input.push(' ');
                            } else if !app.input.is_empty() {
                                let prompt = app.input.clone();
                                app.messages.push(format!("You: {}", prompt));

                                let agent_name = if !app.agents.is_empty() {
                                    app.agents[app.selected_agent].name.clone()
                                } else {
                                    "System".to_string()
                                };

                                // Build the message and route through the bus
                                let bus = app.bus.clone();
                                let user_id = app.user_agent_id;

                                // Resolve the selected agent's bus identity
                                let target_addr = app.runtime.get_agent_id(&agent_name)
                                    .map(|id| AgentAddress {
                                        agent_id: id,
                                        agent_name: agent_name.clone(),
                                        adapter_type: "agent".to_string(),
                                    });

                                let from_addr = AgentAddress {
                                    agent_id: user_id,
                                    agent_name: "You".to_string(),
                                    adapter_type: "tui".to_string(),
                                };

                                let bus_msg = AgentMessage::new(
                                    from_addr,
                                    target_addr,
                                    MessageType::TaskRequest,
                                    Payload::Text(prompt),
                                );

                                tokio::spawn(async move {
                                    if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await {
                                        tracing::error!("Failed to send to bus: {}", e);
                                    }
                                });

                                app.input.clear();
                                app.chat_scroll_offset = 0; // jump to bottom when sending message
                                app.input_scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                }
                // Ignore Event::Mouse, Event::Resize, Event::FocusGained/Lost,
                // and Key events that aren't Press (Release/Repeat).
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = std::time::Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    // Left panel for agents, main panel for chat, bottom panel for input
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
        .split(f.size());

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(7)].as_ref())
        .split(chunks[1]);

    // 1. Agent List — each row shows a colored status glyph, the agent
    // name, and (if applicable) the last latency or live elapsed counter
    // for in-flight requests.  We query `runtime.activity_of` on every
    // tick so Thinking-state elapsed counters animate live.
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let activity = app.runtime.activity_of(&a.name);
            let (glyph, glyph_color, suffix) = render_activity(&activity);

            let selected = i == app.selected_agent;
            let cursor = if selected { "> " } else { "  " };
            let mut name_style = Style::default();
            if selected {
                name_style = name_style.add_modifier(Modifier::BOLD).fg(Color::Cyan);
            }

            let mut spans = vec![
                Span::raw(cursor),
                Span::styled(glyph, Style::default().fg(glyph_color)),
                Span::raw(" "),
                Span::styled(a.name.clone(), name_style),
            ];
            if !suffix.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", suffix),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut agents_block = Block::default().borders(Borders::ALL).title("Agents");
    if app.focused_panel == FocusedPanel::Agents {
        agents_block = agents_block.border_style(Style::default().fg(Color::LightBlue));
    }
    let agents_list = List::new(items).block(agents_block);
    f.render_widget(agents_list, chunks[0]);

    // 2. Chat Messages
    let mut messages_block = Block::default()
        .borders(Borders::ALL)
        .title("Channel: #general");
    if app.focused_panel == FocusedPanel::Chat {
        messages_block = messages_block.border_style(Style::default().fg(Color::LightBlue));
    }
        
    let inner_area = messages_block.inner(right_chunks[0]);
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    let mut all_lines: Vec<String> = Vec::new();
    for m in &app.messages {
        let wrapped = wrap_text(m, width);
        all_lines.extend(wrapped);
    }
    
    // Auto-scroll: take the last `height` lines, adjusted by `chat_scroll_offset`
    let max_scroll = all_lines.len().saturating_sub(height);
    let current_scroll = max_scroll.saturating_sub(app.chat_scroll_offset);
    let visible_lines = &all_lines[current_scroll..current_scroll + height.min(all_lines.len() - current_scroll)];

    let messages_items: Vec<ListItem> = visible_lines
        .iter()
        .map(|m| {
            let content = vec![Line::from(Span::raw(m))];
            ListItem::new(content)
        })
        .collect();

    let messages_list = List::new(messages_items).block(messages_block);
    f.render_widget(messages_list, right_chunks[0]);

    // 3. Input Panel
    let mut input_block = Block::default().borders(Borders::ALL).title("Input (Press Tab to switch focus)");
    if app.focused_panel == FocusedPanel::Input {
        input_block = input_block.border_style(Style::default().fg(Color::LightBlue));
    }
    let input_par = Paragraph::new(app.input.as_str())
        .block(input_block)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((app.input_scroll_offset as u16, 0));
    f.render_widget(input_par, right_chunks[1]);
}

/// Map an `AgentActivity` to (glyph, color, suffix) for the agent list.
///
/// Glyph choice:
///   ●  filled circle — definite status (idle/thinking/errored, all
///      colored differently).  Falls back consistently across most
///      monospace terminal fonts.
///   ○  hollow circle — Unknown (never spoken).
///
/// Suffix:
///   Idle      → last-latency ("780ms" or "2.1s")
///   Thinking  → live elapsed since the request was sent ("12s…")
///   Errored   → "err" (red).  Truncating the actual error keeps the
///               agent list narrow; the full error appears in the chat.
///   Unknown   → empty (clean default for not-yet-spoken agents).
fn render_activity(activity: &AgentActivity) -> (&'static str, Color, String) {
    match activity {
        AgentActivity::Unknown => ("○", Color::DarkGray, String::new()),
        AgentActivity::Idle { last_latency_ms } => {
            let suffix = match last_latency_ms {
                Some(ms) => format_latency(*ms),
                None => String::new(),
            };
            ("●", Color::Green, suffix)
        }
        AgentActivity::Thinking { since } => {
            let elapsed_secs = (chrono::Utc::now() - *since).num_seconds().max(0);
            let suffix = format!("{}s…", elapsed_secs);
            ("●", Color::Yellow, suffix)
        }
        AgentActivity::Errored { .. } => ("✗", Color::Red, "err".to_string()),
    }
}

/// Render a millisecond latency in a compact form: `780ms` for sub-second,
/// `2.1s` for seconds, `4m12s` for minutes (rare but possible for slow
/// CLIs like cold-started Gemini).
fn format_latency(ms: u64) -> String {
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let total_s = ms / 1000;
        format!("{}m{}s", total_s / 60, total_s % 60)
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let text = text.replace('\r', "");
    for paragraph in text.split('\n') {
        let chars: Vec<char> = paragraph.chars().collect();
        if chars.is_empty() {
            lines.push(String::new());
            continue;
        }
        for chunk in chars.chunks(width) {
            lines.push(chunk.iter().collect());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_latency_sub_second() {
        assert_eq!(format_latency(0), "0ms");
        assert_eq!(format_latency(780), "780ms");
        assert_eq!(format_latency(999), "999ms");
    }

    #[test]
    fn format_latency_seconds() {
        assert_eq!(format_latency(1_000), "1.0s");
        assert_eq!(format_latency(2_100), "2.1s");
        assert_eq!(format_latency(59_999), "60.0s");
    }

    #[test]
    fn format_latency_minutes() {
        assert_eq!(format_latency(60_000), "1m0s");
        assert_eq!(format_latency(252_000), "4m12s"); // 4m12s ≈ Gemini cold start
    }

    #[test]
    fn render_activity_unknown_has_empty_suffix() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Unknown);
        assert_eq!(glyph, "○");
        assert_eq!(suffix, "");
    }

    #[test]
    fn render_activity_idle_with_latency() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Idle {
            last_latency_ms: Some(780),
        });
        assert_eq!(glyph, "●");
        assert_eq!(suffix, "780ms");
    }

    #[test]
    fn render_activity_errored_shows_err_suffix() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Errored {
            last_error: "boom".into(),
        });
        assert_eq!(glyph, "✗");
        assert_eq!(suffix, "err");
    }
}
