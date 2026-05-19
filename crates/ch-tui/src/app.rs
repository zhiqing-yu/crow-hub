use anyhow::Result;
use ch_agent::{AgentActivity, AgentInfo, AgentRuntime};
use ch_core::MessageBus;
use ch_memory::MemoryStore;
use ch_protocol::{AgentAddress, AgentId, AgentMessage, MessageType, Payload, MemoryEntry};
use crate::theme::Theme;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, MouseEventKind,
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
use std::collections::HashSet;
use std::sync::Arc;
use std::{io, time::Duration};
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
pub enum FocusedPanel {
    Agents,
    Chat,
    Input,
    Memory,
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
    /// Indices (into `agents`) of agents the user has multi-selected via
    /// Space.  When non-empty, pressing Enter broadcasts the prompt to all
    /// of them in parallel.  When empty, falls back to single-agent send
    /// using `selected_agent`.
    pub multi_selected: HashSet<usize>,
    pub should_quit: bool,
    pub response_rx: mpsc::Receiver<(String, String)>,
    pub tx: mpsc::Sender<(String, String)>,
    pub focused_panel: FocusedPanel,
    pub chat_scroll_offset: usize,
    pub input_scroll_offset: usize,
    /// Frame counter for animated spinners and transitions
    pub tick_count: u64,
    /// Memory store for the TUI panel (read-only, shared with writer)
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    /// Cached memory rows for the current channel
    pub memory_rows: Vec<MemoryEntry>,
    /// Scroll offset for the memory panel
    pub memory_scroll_offset: usize,
    /// Active theme
    pub theme: Theme,
}

impl App {
    pub fn new(
        runtime: Arc<AgentRuntime>,
        bus: Arc<MessageBus>,
        user_agent_id: AgentId,
        tx: mpsc::Sender<(String, String)>,
        response_rx: mpsc::Receiver<(String, String)>,
        memory_store: Option<Arc<dyn MemoryStore>>,
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
            multi_selected: HashSet::new(),
            should_quit: false,
            response_rx,
            tx,
            focused_panel: FocusedPanel::Input,
            chat_scroll_offset: 0,
            input_scroll_offset: 0,
            tick_count: 0,
            memory_store,
            memory_rows: Vec::new(),
            memory_scroll_offset: 0,
            theme: crate::theme::from_env(),
        }
    }

    /// Toggle the currently-cursored agent into / out of the multi-selection.
    /// Bound to Space when the Agents panel is focused.
    pub fn toggle_multi_select_current(&mut self) {
        toggle_multi_select(
            &mut self.multi_selected,
            self.selected_agent,
            self.agents.len(),
        );
    }

    /// Clear the multi-selection.  Bound to Backspace when the Agents
    /// panel is focused (Backspace still deletes input characters on other
    /// panels — context-sensitive).
    pub fn clear_multi_select(&mut self) {
        self.multi_selected.clear();
    }

    /// Resolve the names of the agents that should receive the next
    /// prompt.  If anything is multi-selected, those are the targets
    /// (sorted by index for deterministic order); otherwise fall back to
    /// the single primary cursor.
    pub fn current_send_targets(&self) -> Vec<String> {
        let agent_names: Vec<&str> = self.agents.iter().map(|a| a.name.as_str()).collect();
        resolve_send_targets(&agent_names, &self.multi_selected, self.selected_agent)
    }

    /// Refresh the memory panel from the SQLite store.
    /// Safe to call from a sync context (blocks on tokio handle).
    pub fn refresh_memory(&mut self) {
        if let Some(ref store) = self.memory_store {
            let store = store.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                if let Ok(rows) = handle.block_on(store.recent("general", 50)) {
                    self.memory_rows = rows;
                }
            }
        }
    }

    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
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
    memory_store: Option<Arc<dyn MemoryStore>>,
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
    // Update: User specifically requested mouse scrolling, so we will enable it
    // by default, but allow disabling it via CROW_NO_MOUSE=1 for Antigravity.
    let enable_mouse = std::env::var("CROW_NO_MOUSE")
        .map(|v| v != "1" && v != "true")
        .unwrap_or(true);
    if enable_mouse {
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
    } else {
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new(runtime, bus, user_agent_id, tx, response_rx, memory_store);
    let res = run_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    if enable_mouse {
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableBracketedPaste,
            DisableMouseCapture
        )?;
    } else {
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableBracketedPaste
        )?;
    }

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
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    MouseEventKind::ScrollUp => match app.focused_panel {
                        FocusedPanel::Input => {
                            app.input_scroll_offset = app.input_scroll_offset.saturating_sub(1)
                        }
                        _ => app.chat_scroll_offset = app.chat_scroll_offset.saturating_add(1),
                    },
                    MouseEventKind::ScrollDown => match app.focused_panel {
                        FocusedPanel::Input => {
                            app.input_scroll_offset = app.input_scroll_offset.saturating_add(1)
                        }
                        _ => app.chat_scroll_offset = app.chat_scroll_offset.saturating_sub(1),
                    },
                    _ => {}
                },
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let now = std::time::Instant::now();
                    let is_fast = now.duration_since(last_key_time) < Duration::from_millis(20);
                    last_key_time = now;

                    match key.code {
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.should_quit = true;
                        }
                        KeyCode::Esc => app.should_quit = true,
                        KeyCode::Tab => {
                            app.focused_panel = match app.focused_panel {
                                FocusedPanel::Input => FocusedPanel::Agents,
                                FocusedPanel::Agents => FocusedPanel::Chat,
                                FocusedPanel::Chat => FocusedPanel::Memory,
                                FocusedPanel::Memory => FocusedPanel::Input,
                            };
                            if app.focused_panel == FocusedPanel::Memory {
                                app.refresh_memory();
                            }
                        }
                        KeyCode::BackTab => {
                            app.focused_panel = match app.focused_panel {
                                FocusedPanel::Input => FocusedPanel::Memory,
                                FocusedPanel::Memory => FocusedPanel::Chat,
                                FocusedPanel::Chat => FocusedPanel::Agents,
                                FocusedPanel::Agents => FocusedPanel::Input,
                            };
                            if app.focused_panel == FocusedPanel::Memory {
                                app.refresh_memory();
                            }
                        }
                        KeyCode::Up => match app.focused_panel {
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
                            FocusedPanel::Memory => {
                                app.memory_scroll_offset = app.memory_scroll_offset.saturating_sub(1);
                            }
                        },
                        KeyCode::Down => match app.focused_panel {
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
                            FocusedPanel::Memory => {
                                app.memory_scroll_offset = app.memory_scroll_offset.saturating_add(1);
                            }
                        },
                        KeyCode::Char('r') if app.focused_panel == FocusedPanel::Memory => {
                            app.refresh_memory();
                        }
                        KeyCode::Char(' ') if app.focused_panel == FocusedPanel::Agents => {
                            // Space on Agents panel = toggle multi-select on
                            // the cursored agent.  Pressing Enter while any
                            // agents are multi-selected broadcasts the prompt
                            // to all of them in parallel.
                            app.toggle_multi_select_current();
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        KeyCode::Backspace => {
                            // Context-sensitive: Backspace clears the multi-
                            // selection if the Agents panel is focused; on
                            // any other panel it deletes a character from
                            // the input field.
                            if app.focused_panel == FocusedPanel::Agents {
                                app.clear_multi_select();
                            } else {
                                app.input.pop();
                            }
                        }
                        KeyCode::Enter => {
                            if is_fast {
                                // Un-bracketed paste detection: If Enter arrives too quickly
                                // after another character, treat it as a pasted newline (space).
                                app.input.push(' ');
                            } else if !app.input.is_empty() {
                                let prompt = app.input.clone();
                                app.messages.push(format!("You: {}", prompt));

                                // Resolve targets: multi-selection wins if
                                // non-empty; otherwise single cursor.
                                let targets = app.current_send_targets();

                                let bus = app.bus.clone();
                                let user_id = app.user_agent_id;
                                let runtime = app.runtime.clone();

                                // Fan out: one tokio task per target, all
                                // independent.  The bus dispatches each
                                // message to its addressed agent in parallel.
                                for agent_name in targets {
                                    send_prompt_to_agent(
                                        &runtime,
                                        bus.clone(),
                                        user_id,
                                        agent_name,
                                        prompt.clone(),
                                    );
                                }

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

/// Spawn a tokio task that publishes a single TaskRequest to one agent
/// through the bus.  Extracted from the Enter handler so multi-agent
/// broadcast can call it in a loop, once per selected target.  Each
/// task is independent — the agents respond in parallel.
fn send_prompt_to_agent(
    runtime: &Arc<AgentRuntime>,
    bus: Arc<MessageBus>,
    user_id: AgentId,
    agent_name: String,
    prompt: String,
) {
    let target_addr = runtime.get_agent_id(&agent_name).map(|id| AgentAddress {
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
            tracing::error!("Failed to send to bus for {}: {}", agent_name, e);
        }
    });
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    // Split screen: main area + 1-line footer
    let main_footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)].as_ref())
        .split(f.size());
    let main_area = main_footer[0];
    let footer_area = main_footer[1];

    // Left panel for agents, main panel for chat, bottom panel for input
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(main_area);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(7)].as_ref())
        .split(chunks[1]);

    // 1a. Agent status summary — quick overview at top of panel
    let (mut thinking, mut idle, mut errored, mut unknown) = (0u32, 0u32, 0u32, 0u32);
    for a in &app.agents {
        let act = app.runtime.activity_of(&a.name);
        match act {
            AgentActivity::Thinking { .. } => thinking += 1,
            AgentActivity::Idle { .. } => idle += 1,
            AgentActivity::Errored { .. } => errored += 1,
            AgentActivity::Unknown => unknown += 1,
        }
    }
    let summary = format!(
        "{} thinking  {} ready  {} erred  {} new",
        thinking, idle, errored, unknown
    );

    // 1b. Agent List — each row shows:
    //   `[✓]` / `[ ]`           (multi-select checkbox — visible only when
    //                            ANY agent is multi-selected; otherwise the
    //                            checkbox column collapses to keep clean
    //                            rendering for the common single-agent case)
    //   colored status glyph   (●/◐/✗/○ from render_activity)
    //   agent name
    //   suffix (latency / elapsed / err)
    //
    // We query `runtime.activity_of` on every tick so Thinking-state
    // elapsed counters animate live.
    let any_multi = !app.multi_selected.is_empty();
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let activity = app.runtime.activity_of(&a.name);
            let (glyph, glyph_color, suffix) = render_activity(&activity, app.tick_count, &app.theme);

            let selected = i == app.selected_agent;
            let multi = app.multi_selected.contains(&i);

            // Multi-select column.  Only render when at least one agent is
            // multi-selected, so the single-agent default view stays compact.
            let multi_box: Option<Span> = if any_multi {
                if multi {
                    Some(Span::styled("[✓] ", Style::default().fg(app.theme.agent_multi)))
                } else {
                    Some(Span::styled("[ ] ", Style::default().fg(app.theme.agent_multi_dim)))
                }
            } else {
                None
            };

            let mut name_style = Style::default();
            if multi {
                name_style = name_style.fg(app.theme.agent_multi);
            } else if selected {
                name_style = name_style.add_modifier(Modifier::BOLD).fg(app.theme.agent_cursor);
            }

            let mut spans = Vec::new();
            if let Some(box_span) = multi_box {
                spans.push(box_span);
            }
            spans.push(Span::styled(glyph, Style::default().fg(glyph_color)));
            // Suffix before name — always visible, name truncates instead
            if !suffix.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", suffix),
                    Style::default().fg(app.theme.suffix),
                ));
            }
            spans.push(Span::styled(a.name.clone(), name_style));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut agents_block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Agents  v{}", env!("CARGO_PKG_VERSION")));
    if app.focused_panel == FocusedPanel::Agents {
        agents_block = agents_block.border_style(Style::default().fg(app.theme.border_focused));
    }

    // Render the panel block (border + title) ONCE, then split its inner
    // area into [summary | list].  The previous implementation rendered
    // both the summary AND a separate List-with-block to the full sidebar,
    // which caused the List's re-painted border + first-row content to
    // overlap the summary text — producing the garbled
    //   "○ openclaw-ssh-1eady  0 erred  11 n"
    // first row that DeepSeek's P0 polish (1cecc11) shipped with.
    let inner = agents_block.inner(chunks[0]);
    f.render_widget(agents_block, chunks[0]);

    // Length(1) — the summary is a single line.  Min(1) leaves the rest
    // of the inner area for the agent list.
    let agent_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
        .split(inner);

    let summary_par = Paragraph::new(summary)
        .style(Style::default().fg(app.theme.summary).add_modifier(Modifier::ITALIC))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(summary_par, agent_chunks[0]);

    // NO `.block(...)` here — the border + title were already drawn above.
    // Rendering the list to agent_chunks[1] (the post-summary sub-region)
    // means each agent gets its own row starting BELOW the summary,
    // instead of overlapping it.
    let agents_list = List::new(items);
    f.render_widget(agents_list, agent_chunks[1]);

    // 2. Chat Messages
    let mut messages_block = Block::default()
        .borders(Borders::ALL)
        .title("Channel: #general");
    if app.focused_panel == FocusedPanel::Chat {
        messages_block = messages_block.border_style(Style::default().fg(app.theme.border_focused));
    }

    let inner_area = messages_block.inner(right_chunks[0]);
    let width = inner_area.width as usize;
    let height = inner_area.height as usize;

    if app.focused_panel == FocusedPanel::Memory {
        // ── Memory Panel ────────────────────────────────────────
        let mut mem_block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Memory  (last {}, ↑↓:scroll)", app.memory_rows.len()));
        if app.focused_panel == FocusedPanel::Memory {
            mem_block = mem_block.border_style(Style::default().fg(app.theme.border_focused));
        }

        let mem_items: Vec<ListItem> = app
            .memory_rows
            .iter()
            .rev() // newest at bottom
            .skip(app.memory_scroll_offset)
            .take(height)
            .map(|entry| {
                let glyph = match entry.memory_type.as_str() {
                    "taskrequest" => "→",
                    "taskresponse" => "←",
                    _ => "·",
                };
                let from = entry
                    .metadata
                    .get("from_agent_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        let id = entry.agent_id.to_string();
                        if id.len() >= 8 { id[..8].to_string() } else { id }
                    });
                let ts = entry.created_at.format("%m-%d %H:%M");
                let content = entry.content.replace('\n', " ").replace('\r', " ");
                let line = format!(
                    "{} {} {:>8}  {}",
                    glyph,
                    ts,
                    from,
                    content
                );
                ListItem::new(Line::from(Span::raw(line)))
            })
            .collect();

        let mem_list = List::new(mem_items).block(mem_block);
        f.render_widget(mem_list, right_chunks[0]);
    } else {
        // ── Chat Panel ──────────────────────────────────────────
        let mut all_lines: Vec<String> = Vec::new();
        for m in &app.messages {
            let wrapped = wrap_text(m, width);
            all_lines.extend(wrapped);
        }

        let max_scroll = all_lines.len().saturating_sub(height);
        let current_scroll = max_scroll.saturating_sub(app.chat_scroll_offset);
        let visible_lines =
            &all_lines[current_scroll..current_scroll + height.min(all_lines.len() - current_scroll)];

        let messages_items: Vec<ListItem> = visible_lines
            .iter()
            .map(|m| {
                let content = vec![Line::from(Span::raw(m))];
                ListItem::new(content)
            })
            .collect();

        let messages_list = List::new(messages_items).block(messages_block);
        f.render_widget(messages_list, right_chunks[0]);
    }

    // 3. Input Panel
    let mut input_block = Block::default()
        .borders(Borders::ALL)
        .title("Input (Press Tab to switch focus)");
    if app.focused_panel == FocusedPanel::Input {
        input_block = input_block.border_style(Style::default().fg(app.theme.border_focused));
    }
    let input_par = Paragraph::new(app.input.as_str())
        .block(input_block)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((app.input_scroll_offset as u16, 0));
    f.render_widget(input_par, right_chunks[1]);

    // Footer: keyboard shortcut bar (context-sensitive)
    let shortcuts = match app.focused_panel {
        FocusedPanel::Agents => "↑↓:navigate  Space:multi-select  Backspace:clear  Enter:send  Tab:next",
        FocusedPanel::Chat => "↑↓:scroll  Tab:next  Ctrl+C:quit",
        FocusedPanel::Input => "Enter:send  Tab:next  Ctrl+C:quit",
        FocusedPanel::Memory => "↑↓:scroll  r:refresh  Tab:next  Ctrl+C:quit",
    };
    let footer = Paragraph::new(shortcuts)
        .style(Style::default().fg(app.theme.footer).bg(Color::Black))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(footer, footer_area);
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
/// Braille spinner frames for animated thinking indicator
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn render_activity(activity: &AgentActivity, tick: u64, theme: &Theme) -> (&'static str, Color, String) {
    match activity {
        AgentActivity::Unknown => ("○", theme.status_unknown, String::new()),
        AgentActivity::Idle {
            last_latency_ms,
            cumulative_tokens_in,
            cumulative_tokens_out,
            cumulative_cost_usd,
        } => {
            let mut suffix = match last_latency_ms {
                Some(ms) => format_latency(*ms),
                None => String::new(),
            };
            if *cumulative_tokens_in > 0 || *cumulative_tokens_out > 0 {
                use std::fmt::Write;
                let _ = write!(suffix, "·{}/{}", format_tokens(*cumulative_tokens_in), format_tokens(*cumulative_tokens_out));
            }
            if *cumulative_cost_usd > 0.0 {
                use std::fmt::Write;
                let _ = write!(suffix, "·${:.2}", cumulative_cost_usd);
            }
            ("●", theme.status_idle, suffix)
        }
        AgentActivity::Thinking { since } => {
            let elapsed_secs = (chrono::Utc::now() - *since).num_seconds().max(0);
            let suffix = format!("{}s…", elapsed_secs);
            let glyph = SPINNER[(tick as usize / 3) % SPINNER.len()];
            (glyph, theme.status_thinking, suffix)
        }
        AgentActivity::Errored { .. } => ("✗", theme.status_errored, "err".to_string()),
    }
}

/// Toggle membership of `cursor` in `set` if `cursor` is a valid index
/// into a list of `agent_count` items.  Pure helper extracted from
/// `App::toggle_multi_select_current` so the state mutation can be
/// unit-tested without constructing a full TUI App.
fn toggle_multi_select(set: &mut HashSet<usize>, cursor: usize, agent_count: usize) {
    if cursor >= agent_count {
        return;
    }
    if !set.insert(cursor) {
        set.remove(&cursor);
    }
}

/// Resolve the list of agent names to fan a prompt out to.
///
/// * If `multi_selected` is non-empty: return the names of those agents,
///   sorted by their indices (deterministic order for predictable
///   broadcast behavior and stable test expectations).
/// * Otherwise: return a single-element Vec with the primary-cursor
///   agent (or empty Vec if there are no agents loaded).
///
/// Pure helper extracted from `App::current_send_targets` so the routing
/// logic can be unit-tested without constructing a full TUI App.
fn resolve_send_targets(
    agent_names: &[&str],
    multi_selected: &HashSet<usize>,
    primary_cursor: usize,
) -> Vec<String> {
    if !multi_selected.is_empty() {
        let mut indices: Vec<usize> = multi_selected.iter().copied().collect();
        indices.sort();
        return indices
            .into_iter()
            .filter_map(|i| agent_names.get(i).map(|s| s.to_string()))
            .collect();
    }
    if !agent_names.is_empty() && primary_cursor < agent_names.len() {
        return vec![agent_names[primary_cursor].to_string()];
    }
    Vec::new()
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

/// Format token count in compact form: 22279 → "22k", 284 → "0.3k".
/// Returns empty string for 0 (caller should skip suffix).
fn format_tokens(n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    if n < 1000 {
        return n.to_string();
    }
    if n < 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{}k", n / 1000)
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
        let (glyph, _, suffix) = render_activity(&AgentActivity::Unknown, 0, &crate::theme::from_env());
        assert_eq!(glyph, "○");
        assert_eq!(suffix, "");
    }

    #[test]
    fn render_activity_idle_with_latency() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Idle {
            last_latency_ms: Some(780),
            cumulative_tokens_in: 0,
            cumulative_tokens_out: 0,
            cumulative_cost_usd: 0.0,
        }, 0, &crate::theme::from_env());
        assert_eq!(glyph, "●");
        assert_eq!(suffix, "780ms");
    }

    #[test]
    fn render_activity_idle_with_tokens() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Idle {
            last_latency_ms: Some(18_600),
            cumulative_tokens_in: 22279,
            cumulative_tokens_out: 284,
            cumulative_cost_usd: 0.0,
        }, 0, &crate::theme::from_env());
        assert_eq!(glyph, "●");
        assert_eq!(suffix, "18.6s·22k/284");
    }

    #[test]
    fn render_activity_idle_no_tokens_when_zero() {
        let (_, _, suffix) = render_activity(&AgentActivity::Idle {
            last_latency_ms: Some(100),
            cumulative_tokens_in: 0,
            cumulative_tokens_out: 0,
            cumulative_cost_usd: 0.0,
        }, 0, &crate::theme::from_env());
        assert!(!suffix.contains("·"), "no token suffix when both zero");
    }

    #[test]
    fn format_tokens_compact() {
        assert_eq!(format_tokens(0), "");
        assert_eq!(format_tokens(284), "284");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(22279), "22k");
        assert_eq!(format_tokens(1500), "1.5k");
    }

    #[test]
    fn render_activity_errored_shows_err_suffix() {
        let (glyph, _, suffix) = render_activity(&AgentActivity::Errored {
            last_error: "boom".into(),
        }, 0, &crate::theme::from_env());
        assert_eq!(glyph, "✗");
        assert_eq!(suffix, "err");
    }

    // ── multi-select helpers ────────────────────────────────────

    #[test]
    fn toggle_multi_select_adds_then_removes() {
        let mut set = HashSet::new();
        toggle_multi_select(&mut set, 1, 4);
        assert!(set.contains(&1));
        // Toggling the same index removes it.
        toggle_multi_select(&mut set, 1, 4);
        assert!(!set.contains(&1));
    }

    #[test]
    fn toggle_multi_select_ignores_out_of_bounds() {
        let mut set = HashSet::new();
        toggle_multi_select(&mut set, 7, 4);
        assert!(set.is_empty(), "cursor past agent_count must be a no-op");
        toggle_multi_select(&mut set, 4, 4);
        assert!(set.is_empty(), "cursor == agent_count must also be no-op");
    }

    #[test]
    fn toggle_multi_select_is_noop_when_no_agents() {
        let mut set = HashSet::new();
        toggle_multi_select(&mut set, 0, 0);
        assert!(set.is_empty());
    }

    #[test]
    fn resolve_send_targets_falls_back_to_primary_when_no_multi_select() {
        let agents = vec!["a", "b", "c"];
        let set = HashSet::new();
        let targets = resolve_send_targets(&agents, &set, 1);
        assert_eq!(targets, vec!["b".to_string()]);
    }

    #[test]
    fn resolve_send_targets_returns_empty_when_no_agents_at_all() {
        let agents: Vec<&str> = vec![];
        let set = HashSet::new();
        let targets = resolve_send_targets(&agents, &set, 0);
        assert!(targets.is_empty());
    }

    #[test]
    fn resolve_send_targets_returns_multi_in_sorted_order() {
        let agents = vec!["a", "b", "c", "d"];
        let mut set = HashSet::new();
        // Insert out of order to verify the helper sorts indices.
        set.insert(2);
        set.insert(0);
        set.insert(3);
        let targets = resolve_send_targets(&agents, &set, 1);
        assert_eq!(
            targets,
            vec!["a".to_string(), "c".to_string(), "d".to_string()]
        );
    }

    #[test]
    fn resolve_send_targets_multi_select_overrides_primary_cursor() {
        // When multi_selected is non-empty, the primary cursor index is
        // IGNORED — only multi-selected indices contribute.  This keeps
        // the UX predictable: "if I've selected some, those are who I'm
        // talking to, regardless of where my cursor wanders."
        let agents = vec!["a", "b", "c"];
        let mut set = HashSet::new();
        set.insert(0);
        let targets = resolve_send_targets(&agents, &set, 2); // cursor on "c"
        assert_eq!(targets, vec!["a".to_string()]);
    }

    #[test]
    fn resolve_send_targets_filters_indices_past_end() {
        // Defensive: if the multi-selection contains a stale index
        // (e.g. agents shrank), we silently drop it instead of panicking.
        let agents = vec!["a", "b"];
        let mut set = HashSet::new();
        set.insert(0);
        set.insert(5); // stale
        let targets = resolve_send_targets(&agents, &set, 0);
        assert_eq!(targets, vec!["a".to_string()]);
    }
}
