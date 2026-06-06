use crate::theme::Theme;
use anyhow::Result;
use ch_agent::{AgentActivity, AgentInfo, AgentRuntime};
use ch_core::MessageBus;
use ch_memory::MemoryStore;
use ch_protocol::{
    AgentAddress, AgentId, AgentMessage, EvidenceClaim, HandoffEnvelope, MemoryEntry, MessageType,
    Payload, WorkflowClaimMsg,
};
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

/// Every slash command the TUI implements.  The `help_lines()` output and
/// the `handle_slash_command` match arms MUST stay in sync with this list —
/// a regression test in `tests` asserts every entry here appears in
/// `help_lines()`, so adding a new command without updating `/help` will
/// fail CI rather than silently leaving users without docs.
pub const SUPPORTED_COMMANDS: &[&str] = &[
    "/clear",
    "/model",
    "/all",
    "/handoff",
    "/evidence",
    "/workflow",
    "/help",
];

#[derive(Debug, PartialEq, Eq)]
pub enum FocusedPanel {
    Agents,
    Chat,
    Input,
    Memory,
}

/// Top-level views in the TUI.  Each tab is a different lens on the same
/// underlying state.
///
/// Naming note: `Home` is the dashboard view (agent activity summary,
/// status glyphs, recent throughput).  The label "Agents" is intentionally
/// reserved for a future management view (register / unregister / configure
/// CLI clients) so we don't conflate "look at agent status" with "edit the
/// agent roster."
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Tab {
    Home,
    Chat,
    Monitor,
    Memory,
}

/// App state
pub struct App {
    pub runtime: Arc<AgentRuntime>,
    pub bus: Arc<MessageBus>,
    pub user_agent_id: AgentId,
    pub agents: Vec<AgentInfo>,
    pub input: String,
    pub cursor_pos: usize,
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
    /// Pending memory refresh result (populated by background task)
    pub pending_memory: Arc<std::sync::Mutex<Option<Vec<MemoryEntry>>>>,
    /// Scroll offset for the memory panel
    pub memory_scroll_offset: usize,
    /// Active theme
    pub theme: Theme,
    pub default_model: String,
    /// Tab bar selection
    pub active_tab: Tab,
    /// /all forces global view even when single agent selected
    pub chat_scope_all: bool,
    pub show_help_overlay: bool,
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
            cursor_pos: 0,
            chat_scroll_offset: 0,
            input_scroll_offset: 0,
            tick_count: 0,
            memory_store,
            memory_rows: Vec::new(),
            pending_memory: Arc::new(std::sync::Mutex::new(None)),
            memory_scroll_offset: 0,
            theme: crate::theme::from_env(),
            default_model: String::new(),
            active_tab: Tab::Chat,
            chat_scope_all: false,
            show_help_overlay: false,
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

    /// Kick off a background memory refresh.
    pub fn refresh_memory(&mut self) {
        if let Some(ref store) = self.memory_store {
            let store = store.clone();
            let pending = self.pending_memory.clone();
            tokio::spawn(async move {
                if let Ok(rows) = store.recent("general", 50).await {
                    *pending.lock().unwrap() = Some(rows);
                }
            });
        }
    }

    /// Collect results from a background memory refresh.
    pub fn collect_memory_refresh(&mut self) {
        if let Some(rows) = self.pending_memory.lock().unwrap().take() {
            self.memory_rows = rows;
        }
    }

    /// Handle a slash command.  Returns true if input was consumed.
    pub fn handle_slash_command(&mut self) -> bool {
        let cmd = self.input.trim().to_string();
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let verb = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match verb.as_str() {
            "/clear" => {
                self.messages.clear();
                self.messages.push("Chat cleared.".into());
                self.chat_scroll_offset = 0;
            }
            "/model" => {
                if arg.is_empty() {
                    let current = if self.default_model.is_empty() {
                        "none"
                    } else {
                        &self.default_model
                    };
                    self.messages.push(format!(
                        "/model — current: {}  (usage: /model <name>)",
                        current
                    ));
                } else {
                    self.default_model = arg.to_string();
                    self.messages
                        .push(format!("/model — default model set to: {}", arg));
                }
            }
            "/all" => {
                self.chat_scope_all = true;
                self.messages
                    .push("── Showing all agents (/all off — switch agent to scope) ──".into());
                self.chat_scroll_offset = 0;
            }
            "/handoff" => {
                // Manual handoff envelope emission.  Useful for testing the
                // Handoff data path end-to-end before agents auto-emit them
                // from prompt output.  Broadcasts as MessageType::Handoff +
                // Payload::Handoff on the "general" channel — every subscribed
                // agent (and the memory writer) receives it.
                if arg.is_empty() {
                    self.messages
                        .push("/handoff — usage: /handoff <summary>".into());
                } else {
                    let envelope = HandoffEnvelope {
                        from_agent: "You".to_string(),
                        summary: arg.to_string(),
                        ..Default::default()
                    };
                    // Render in the chat panel so the user sees feedback.
                    // The `⇄` glyph distinguishes handoffs from regular text;
                    // the chat scope filter passes any line starting with `⇄`
                    // unconditionally so handoffs stay visible across scopes.
                    self.messages.push(format!("⇄ You → {}", arg));

                    // Fire-and-forget bus send.  Same pattern as
                    // send_prompt_to_agent.  Broadcast (to: None) so the
                    // memory writer and any subscribed agent receives it.
                    let bus = self.bus.clone();
                    let user_id = self.user_agent_id;
                    let from_addr = AgentAddress {
                        agent_id: user_id,
                        agent_name: "You".to_string(),
                        adapter_type: "tui".to_string(),
                    };
                    let bus_msg = AgentMessage::new(
                        from_addr,
                        None,
                        MessageType::Handoff,
                        Payload::Handoff(envelope),
                    );
                    tokio::spawn(async move {
                        if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await {
                            tracing::error!("Failed to send handoff to bus: {}", e);
                        }
                    });
                }
            }
            "/evidence" => {
                // Manual evidence claim emission.  Syntax:
                //   /evidence claim <text>
                // Builds an EvidenceClaim with task_id defaulted to the TUI
                // session's user_agent_id (so claims from the same TUI session
                // group together), broadcasts on the bus.  The memory writer
                // persists this to the `evidence` table with status=pending;
                // `crow memory evidence` lists them.
                let (subcmd, rest) = match arg.split_once(' ') {
                    Some((s, r)) => (s.trim(), r.trim()),
                    None => (arg, ""),
                };
                match subcmd {
                    "claim" if !rest.is_empty() => {
                        let claim = EvidenceClaim {
                            task_id: self.user_agent_id.to_string(),
                            claim: rest.to_string(),
                            witness: None,
                        };
                        // Local feedback line.  `📋` glyph distinguishes evidence
                        // from handoffs (⇄) and regular chat; the chat scope
                        // filter passes any line starting with `📋`.
                        self.messages.push(format!("📋 You → claimed: {}", rest));

                        let bus = self.bus.clone();
                        let user_id = self.user_agent_id;
                        let from_addr = AgentAddress {
                            agent_id: user_id,
                            agent_name: "You".to_string(),
                            adapter_type: "tui".to_string(),
                        };
                        let bus_msg = AgentMessage::new(
                            from_addr,
                            None,
                            MessageType::Evidence,
                            Payload::Evidence(claim),
                        );
                        tokio::spawn(async move {
                            if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await
                            {
                                tracing::error!("Failed to send evidence to bus: {}", e);
                            }
                        });
                    }
                    "claim" => {
                        self.messages
                            .push("/evidence claim — usage: /evidence claim <text>".into());
                    }
                    "verify" if !rest.is_empty() => {
                        let id = rest.to_string();
                        self.messages.push(format!("📋 You → verified: {}", id));
                        let bus = self.bus.clone();
                        let user_id = self.user_agent_id;
                        let from_name = "You".to_string();
                        let bus_msg = AgentMessage::new(
                            AgentAddress {
                                agent_id: user_id,
                                agent_name: from_name,
                                adapter_type: "tui".into(),
                            },
                            None,
                            MessageType::EvidenceVerify,
                            Payload::EvidenceVerify(ch_protocol::EvidenceVerifyMsg {
                                evidence_id: id,
                                outcome: true,
                                // verifier tracked via from.agent_name
                                note: Some("verified manually in TUI".to_string()),
                            }),
                        );
                        tokio::spawn(async move {
                            if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await
                            {
                                tracing::error!("Failed to send evidence verify to bus: {}", e);
                            }
                        });
                    }
                    "verify" => {
                        self.messages
                            .push("/evidence verify — usage: /evidence verify <id>".into());
                    }
                    "fail" if !rest.is_empty() => {
                        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                        let id = parts[0].to_string();
                        let reason = parts
                            .get(1)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "failed manually in TUI".to_string());
                        self.messages
                            .push(format!("📋 You → failed: {} ({})", id, reason));
                        let bus = self.bus.clone();
                        let user_id = self.user_agent_id;
                        let from_name = "You".to_string();
                        let bus_msg = AgentMessage::new(
                            AgentAddress {
                                agent_id: user_id,
                                agent_name: from_name,
                                adapter_type: "tui".into(),
                            },
                            None,
                            MessageType::EvidenceVerify,
                            Payload::EvidenceVerify(ch_protocol::EvidenceVerifyMsg {
                                evidence_id: id,
                                outcome: false,
                                // verifier tracked via from.agent_name
                                note: Some(reason),
                            }),
                        );
                        tokio::spawn(async move {
                            if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await
                            {
                                tracing::error!("Failed to send evidence fail to bus: {}", e);
                            }
                        });
                    }
                    "fail" => {
                        self.messages
                            .push("/evidence fail — usage: /evidence fail <id> <reason>".into());
                    }
                    "" => {
                        self.messages
                            .push("/evidence — usage: /evidence claim|verify|fail ...".into());
                    }
                    other => {
                        self.messages.push(format!(
                            "/evidence — unknown sub-command '{}' (try: claim, verify, fail)",
                            other
                        ));
                    }
                }
            }
            "/workflow" => {
                // Manual workflow step claim.  Syntax:
                //   /workflow claim <step_id>
                // Broadcasts WorkflowClaim on the bus; the memory writer
                // persists it to `workflow_steps` via claim_step().
                // `crow memory workflow` lists them.
                let (subcmd, rest) = match arg.split_once(' ') {
                    Some((s, r)) => (s.trim(), r.trim()),
                    None => (arg, ""),
                };
                match subcmd {
                    "claim" if !rest.is_empty() => {
                        let step_id = rest.to_string();
                        // Local feedback.  `🪧` distinguishes workflow steps
                        // from evidence (📋) and handoffs (⇄); the chat scope
                        // filter passes any line starting with `🪧`.
                        self.messages
                            .push(format!("🪧 You → claimed step {}", step_id));

                        let bus = self.bus.clone();
                        let user_id = self.user_agent_id;
                        let claim = WorkflowClaimMsg {
                            step_id: step_id.clone(),
                            workflow_id: "tui-session".to_string(),
                            agent_id: user_id.to_string(),
                        };
                        let bus_msg = AgentMessage::new(
                            AgentAddress {
                                agent_id: user_id,
                                agent_name: "You".to_string(),
                                adapter_type: "tui".to_string(),
                            },
                            None,
                            MessageType::WorkflowClaim,
                            Payload::WorkflowClaim(claim),
                        );
                        tokio::spawn(async move {
                            if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await
                            {
                                tracing::error!("Failed to send workflow claim to bus: {}", e);
                            }
                        });
                    }
                    "claim" => {
                        self.messages
                            .push("/workflow claim — usage: /workflow claim <step_id>".into());
                    }
                    "" => {
                        self.messages
                            .push("/workflow — usage: /workflow claim <step_id>".into());
                    }
                    other => {
                        self.messages.push(format!(
                            "/workflow — unknown sub-command '{}' (try: claim)",
                            other
                        ));
                    }
                }
            }
            "/help" => {
                for line in help_lines() {
                    self.messages.push(line);
                }
            }
            _ => {
                self.messages
                    .push(format!("Unknown command: {}  (try /help)", verb));
            }
        }
        self.input.clear();
        self.chat_scroll_offset = 0;
        true
    }

    fn input_line_count(&self) -> usize {
        if self.input.is_empty() {
            return 1;
        }
        wrap_text(&self.input, 60).len().max(1)
    }

    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.collect_memory_refresh();
        while let Ok((agent, response)) = self.response_rx.try_recv() {
            append_chat_message(&mut self.messages, &agent, &response);
        }
    }
}

/// Append one `(agent, response)` tuple from the response bridge into the
/// chat message log.  Extracted into a free function so it can be unit-tested
/// without constructing a full `App` (which requires real `AgentRuntime` and
/// `MessageBus` instances).  Two special cases:
///
/// * `agent == "__handoff__"` — the response is a pre-formatted handoff line
///   (e.g. `"⇄ claude → done"`); push as-is so the leading `⇄` glyph survives
///   and the chat scope filter passes it.
/// * `agent` matches the prefix of the last message — streaming continuation;
///   append to the existing line rather than starting a new one.
pub(crate) fn append_chat_message(messages: &mut Vec<String>, agent: &str, response: &str) {
    if agent == "__handoff__" {
        messages.push(response.to_string());
        return;
    }
    if let Some(last_msg) = messages.last_mut() {
        let prefix = format!("{}: ", agent);
        if last_msg.starts_with(&prefix) {
            last_msg.push_str(response);
            return;
        }
    }
    messages.push(format!("{}: {}", agent, response));
}

/// Lines rendered by `/help`.  Extracted as a free function so a unit test
/// can assert every entry in `SUPPORTED_COMMANDS` is documented without
/// constructing a full `App` (which requires real `AgentRuntime` + bus).
///
/// Order: slash commands first, then keyboard shortcuts, separated by a
/// header line each.  Order within slash commands matches `SUPPORTED_COMMANDS`
/// so the regression test catches accidental reorderings too.
pub(crate) fn help_lines() -> Vec<String> {
    vec![
        "┈┈┈ Slash Commands ┈┈┈".to_string(),
        "/clear              Clear chat history".to_string(),
        "/model <name>       Override model for next messages (blank to show)".to_string(),
        "/all                Unscope chat (show all agents)".to_string(),
        "/handoff <summary>  Emit a Handoff envelope on the bus".to_string(),
        "/evidence claim <text>   Emit an Evidence claim".to_string(),
        "/evidence verify <id>    Verify evidence (manual)".to_string(),
        "/evidence fail <id> <r>  Fail evidence (manual)".to_string(),
        "/workflow claim <step_id>  Claim a workflow step".to_string(),
        "/help               Show this help".to_string(),
        "┈┈┈ Keyboard Shortcuts ┈┈┈".to_string(),
        "Tab         Switch panel (Agents→Chat→Memory→Input)".to_string(),
        "↑↓          Navigate / scroll".to_string(),
        "Space       Multi-select agent (Agents panel)".to_string(),
        "Enter       Send message".to_string(),
        "Ctrl+C      Quit".to_string(),
    ]
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

    // Force a clear before first draw — some terminals delay
    // alternate-screen activation by one frame.
    terminal.clear()?;

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
                        KeyCode::Char('?') if app.focused_panel != FocusedPanel::Input => {
                            app.show_help_overlay = !app.show_help_overlay;
                        }
                        KeyCode::Esc if app.show_help_overlay => {
                            app.show_help_overlay = false;
                        }
                        KeyCode::Esc => app.should_quit = true,
                        KeyCode::Tab => {
                            // Tab bar: cycle through tabs
                            app.active_tab = match app.active_tab {
                                Tab::Home => Tab::Chat,
                                Tab::Chat => Tab::Monitor,
                                Tab::Monitor => Tab::Memory,
                                Tab::Memory => Tab::Home,
                            };
                            // Auto-focus appropriate panel
                            app.focused_panel = match app.active_tab {
                                Tab::Home => FocusedPanel::Agents,
                                _ => FocusedPanel::Input,
                            };
                            if app.active_tab == Tab::Memory {
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
                            if app.active_tab == Tab::Memory {
                                app.refresh_memory();
                            }
                        }
                        KeyCode::Up => match app.focused_panel {
                            FocusedPanel::Agents => {
                                if app.selected_agent > 0 {
                                    app.selected_agent -= 1;
                                    app.chat_scope_all = false;
                                }
                            }
                            FocusedPanel::Chat => {
                                app.chat_scroll_offset = app.chat_scroll_offset.saturating_add(1);
                            }
                            FocusedPanel::Input => {
                                if app.input_line_count() > 7 {
                                    app.input_scroll_offset =
                                        app.input_scroll_offset.saturating_sub(1);
                                }
                            }
                            FocusedPanel::Memory => {
                                app.memory_scroll_offset =
                                    app.memory_scroll_offset.saturating_sub(1);
                            }
                        },
                        KeyCode::Down => match app.focused_panel {
                            FocusedPanel::Agents => {
                                if app.selected_agent + 1 < app.agents.len() {
                                    app.selected_agent += 1;
                                    app.chat_scope_all = false;
                                }
                            }
                            FocusedPanel::Chat => {
                                app.chat_scroll_offset = app.chat_scroll_offset.saturating_sub(1);
                            }
                            FocusedPanel::Input => {
                                if app.input_line_count() > 7 {
                                    app.input_scroll_offset =
                                        app.input_scroll_offset.saturating_add(1);
                                }
                            }
                            FocusedPanel::Memory => {
                                app.memory_scroll_offset =
                                    app.memory_scroll_offset.saturating_add(1);
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
                                app.input.push(' ');
                            } else if app.input.starts_with('/') {
                                app.handle_slash_command();
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
                                let mo = if app.default_model.is_empty() {
                                    None
                                } else {
                                    Some(app.default_model.clone())
                                };
                                for agent_name in targets {
                                    send_prompt_to_agent(
                                        &runtime,
                                        bus.clone(),
                                        user_id,
                                        agent_name,
                                        prompt.clone(),
                                        mo.clone(),
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
    model_override: Option<String>,
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
    let mut bus_msg = AgentMessage::new(
        from_addr,
        target_addr,
        MessageType::TaskRequest,
        Payload::Text(prompt),
    );
    if let Some(m) = model_override {
        bus_msg = bus_msg.with_model_override(m);
    }

    tokio::spawn(async move {
        if let Err(e) = bus.send_to_channel("general", &user_id, bus_msg).await {
            tracing::error!("Failed to send to bus for {}: {}", agent_name, e);
        }
    });
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    // Split screen: tab bar + main area + footer
    let tab_main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.size());
    let tab_area = tab_main[0];
    let main_area = tab_main[1];
    let footer_area = tab_main[2];

    // Left panel for agents, main panel for chat, bottom panel for input
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(main_area);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(7)].as_ref())
        .split(chunks[1]);

    // ── Tab Bar ─────────────────────────────────────────────────
    // "Home" = the dashboard view (status of all agents).  The label
    // "Agents" is reserved for a future management tab (register /
    // unregister / configure CLI clients) — see follow-up note in
    // docs/plans/2026-05-24_tui_polish.md.
    let tabs = [
        ("Home", Tab::Home),
        ("Chat", Tab::Chat),
        ("Monitor", Tab::Monitor),
        ("Memory", Tab::Memory),
    ];
    let tab_spans: Vec<Span> = tabs
        .iter()
        .map(|(label, tab)| {
            if app.active_tab == *tab {
                Span::styled(
                    format!(" {} ", label),
                    Style::default()
                        .fg(app.theme.tab_active_text)
                        .bg(app.theme.border_focused)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!(" {} ", label),
                    Style::default().fg(app.theme.tab_inactive),
                )
            }
        })
        .collect();
    // Pad version to the right edge of the tab bar
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tab_width = tab_area.width as usize;
    let used: usize = tab_spans.iter().map(|s| s.width()).sum();
    let pad = tab_width.saturating_sub(used + version.len());
    let mut final_spans = tab_spans;
    if pad > 0 {
        final_spans.push(Span::raw(" ".repeat(pad)));
    }
    final_spans.push(Span::styled(
        version,
        Style::default().fg(app.theme.agent_meta),
    ));
    f.render_widget(
        Paragraph::new(Line::from(final_spans)).style(Style::default().bg(app.theme.surface)),
        tab_area,
    );

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
    //   status glyph (●/◐/✗/○) OR ✓ when multi-selected
    //   agent name
    //   suffix (latency / elapsed / err)
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let activity = app.runtime.activity_of(&a.name);
            let (glyph, glyph_color, suffix) =
                render_activity(&activity, app.tick_count, &app.theme);

            let selected = i == app.selected_agent;
            let multi = app.multi_selected.contains(&i);

            // Multi-selected: swap glyph to ✓ (no `[✓]` wrapper)
            let (display_glyph, display_color) = if multi {
                ("✓", app.theme.agent_multi)
            } else {
                (glyph, glyph_color)
            };

            let mut name_style = Style::default();
            if multi {
                name_style = name_style.fg(app.theme.agent_multi);
            } else if selected {
                name_style = name_style
                    .add_modifier(Modifier::BOLD)
                    .fg(app.theme.agent_cursor);
            }

            let mut spans = Vec::new();
            spans.push(Span::styled(
                display_glyph,
                Style::default().fg(display_color),
            ));
            if !suffix.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", suffix),
                    Style::default().fg(app.theme.suffix),
                ));
            }
            spans.push(Span::styled(a.name.clone(), name_style));
            // Second line: model name in dim text (if known)
            let model_line = a.default_model.as_ref().map(|m| {
                Line::from(Span::styled(
                    format!("   {}", m),
                    Style::default().fg(app.theme.agent_meta),
                ))
            });
            if let Some(ml) = model_line {
                ListItem::new(vec![Line::from(spans), ml])
            } else {
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    let mut agents_block = Block::default().borders(Borders::ALL).title("Agents");
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
        .style(
            Style::default()
                .fg(app.theme.summary)
                .add_modifier(Modifier::ITALIC),
        )
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

    match app.active_tab {
        Tab::Memory => {
            // ── Memory Panel ────────────────────────────────────────
            let mem_block = Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Memory  (last {}, ↑↓:scroll)",
                    app.memory_rows.len()
                ))
                .border_style(Style::default().fg(app.theme.border_focused));

            let mem_items: Vec<ListItem> = app
                .memory_rows
                .iter()
                .rev()
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
                            if id.len() >= 8 {
                                id[..8].to_string()
                            } else {
                                id
                            }
                        });
                    let ts = entry.created_at.format("%m-%d %H:%M");
                    let content = entry.content.replace('\n', " ").replace('\r', " ");
                    let line = format!("{} {} {:>8}  {}", glyph, ts, from, content);
                    ListItem::new(Line::from(Span::raw(line)))
                })
                .collect();

            let mem_list = List::new(mem_items).block(mem_block);
            f.render_widget(mem_list, right_chunks[0]);
        }
        Tab::Monitor => {
            // ── Monitor Panel — detailed per-agent dashboard ────────
            let monitor_block = Block::default()
                .borders(Borders::ALL)
                .title("Monitor — Agent Activity")
                .border_style(Style::default().fg(app.theme.border_focused));

            let monitor_inner = monitor_block.inner(right_chunks[0]);
            f.render_widget(monitor_block, right_chunks[0]);

            let mut rows: Vec<ListItem> = Vec::new();
            // Header
            rows.push(ListItem::new(Line::from(vec![Span::styled(
                format!(
                    "{:<22} {:>6} {:>8} {:>10} {:>10} {:>8}",
                    "Agent", "Status", "Latency", "Tok In", "Tok Out", "Cost"
                ),
                Style::default()
                    .fg(app.theme.agent_meta)
                    .add_modifier(Modifier::BOLD),
            )])));
            rows.push(ListItem::new(Line::from(Span::styled(
                "─".repeat(monitor_inner.width as usize),
                Style::default().fg(app.theme.agent_meta),
            ))));

            for a in &app.agents {
                let activity = app.runtime.activity_of(&a.name);
                let (glyph, glyph_color, _suffix) =
                    render_activity(&activity, app.tick_count, &app.theme);

                let (status_label, latency, tok_in, tok_out, cost) = match &activity {
                    AgentActivity::Unknown => {
                        ("new", "—".into(), "—".into(), "—".into(), "—".into())
                    }
                    AgentActivity::Idle {
                        last_latency_ms,
                        cumulative_tokens_in,
                        cumulative_tokens_out,
                        cumulative_cost_usd,
                    } => (
                        "ready",
                        last_latency_ms
                            .map(format_latency)
                            .unwrap_or_else(|| "—".into()),
                        if *cumulative_tokens_in > 0 {
                            format_tokens(*cumulative_tokens_in)
                        } else {
                            "—".into()
                        },
                        if *cumulative_tokens_out > 0 {
                            format_tokens(*cumulative_tokens_out)
                        } else {
                            "—".into()
                        },
                        if *cumulative_cost_usd > 0.0 {
                            format!("${:.2}", cumulative_cost_usd)
                        } else {
                            "—".into()
                        },
                    ),
                    AgentActivity::Thinking { since } => {
                        let elapsed = (chrono::Utc::now() - *since).num_seconds().max(0);
                        (
                            "think",
                            format!("{}s…", elapsed),
                            "—".into(),
                            "—".into(),
                            "—".into(),
                        )
                    }
                    AgentActivity::Errored { last_error } => {
                        let err_short: String = last_error.chars().take(30).collect();
                        ("error", err_short, "—".into(), "—".into(), "—".into())
                    }
                };

                let multi = app.multi_selected.contains(
                    &app.agents
                        .iter()
                        .position(|x| x.name == a.name)
                        .unwrap_or(usize::MAX),
                );
                let name_color = if multi {
                    app.theme.agent_multi
                } else {
                    app.theme.agent_cursor
                };

                rows.push(ListItem::new(Line::from(vec![
                    Span::styled(glyph, Style::default().fg(glyph_color)),
                    Span::styled(format!("{:<21}", a.name), Style::default().fg(name_color)),
                    Span::styled(
                        format!(" {:>6}", status_label),
                        Style::default().fg(glyph_color),
                    ),
                    Span::styled(
                        format!(" {:>8}", latency),
                        Style::default().fg(app.theme.suffix),
                    ),
                    Span::styled(
                        format!(" {:>10}", tok_in),
                        Style::default().fg(app.theme.agent_meta),
                    ),
                    Span::styled(
                        format!(" {:>10}", tok_out),
                        Style::default().fg(app.theme.agent_meta),
                    ),
                    Span::styled(
                        format!(" {:>8}", cost),
                        Style::default().fg(app.theme.agent_meta),
                    ),
                ])));

                // Second line: model + driver info
                if let Some(ref model) = a.default_model {
                    rows.push(ListItem::new(Line::from(Span::styled(
                        format!("  └ model: {}", model),
                        Style::default().fg(app.theme.agent_meta),
                    ))));
                }
            }

            let monitor_list = List::new(rows);
            f.render_widget(monitor_list, monitor_inner);
        }
        _ => {
            // ── Chat Panel (Agents + Chat tabs) ─────────────────────
            let scope_agent = if app.chat_scope_all || !app.multi_selected.is_empty() {
                None
            } else if !app.agents.is_empty() {
                app.agents.get(app.selected_agent).map(|a| a.name.as_str())
            } else {
                None
            };

            let mut all_lines: Vec<String> = Vec::new();
            for m in &app.messages {
                let show = match scope_agent {
                    Some(name) => {
                        m.starts_with("You:")
                            || m.starts_with("⇄")
                            || m.starts_with("📋")
                            || m.starts_with("🪧")
                            || m.starts_with(&format!("{}:", name))
                    }
                    None => true,
                };
                if show {
                    let wrapped = wrap_text(m, width);
                    all_lines.extend(wrapped);
                }
            }

            let max_scroll = all_lines.len().saturating_sub(height);
            let current_scroll = max_scroll.saturating_sub(app.chat_scroll_offset);
            let visible_lines = &all_lines
                [current_scroll..current_scroll + height.min(all_lines.len() - current_scroll)];

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
    }

    // 3. Input Panel
    let mut input_block = Block::default()
        .borders(Borders::ALL)
        .title("Input (Press Tab to switch focus)");
    if app.focused_panel == FocusedPanel::Input {
        input_block = input_block.border_style(Style::default().fg(app.theme.border_focused));
    }
    // Build input line with cursor indicator
    let input_line = {
        let text = app.input.as_str();
        let pos = app.cursor_pos.min(text.len());
        let mut spans: Vec<Span> = Vec::new();
        if pos > 0 {
            spans.push(Span::raw(text[..pos].to_string()));
        }
        if pos < text.len() {
            spans.push(Span::styled(
                text[pos..pos + 1].to_string(),
                Style::default().bg(app.theme.summary).fg(app.theme.surface),
            ));
            if pos + 1 < text.len() {
                spans.push(Span::raw(text[pos + 1..].to_string()));
            }
        } else if app.tick_count % 2 == 0 {
            // Blinking cursor at end of input
            spans.push(Span::styled(" ", Style::default().bg(app.theme.summary)));
        }
        Line::from(spans)
    };
    let input_par = Paragraph::new(input_line)
        .block(input_block)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((app.input_scroll_offset as u16, 0));
    f.render_widget(input_par, right_chunks[1]);

    // Footer: keyboard shortcut bar (context-sensitive)
    let shortcuts = match app.focused_panel {
        FocusedPanel::Agents => {
            "↑↓:navigate  Space:multi-select  Backspace:clear  Enter:send  Tab:next"
        }
        FocusedPanel::Chat => "↑↓:scroll  Tab:next  Ctrl+C:quit",
        FocusedPanel::Input => "Enter:send  Tab:next  Ctrl+C:quit",
        FocusedPanel::Memory => "↑↓:scroll  r:refresh  Tab:next  Ctrl+C:quit",
    };
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ]
            .as_ref(),
        )
        .split(footer_area);
    f.render_widget(
        Paragraph::new(shortcuts)
            .style(Style::default().fg(app.theme.footer).bg(app.theme.surface)),
        footer_chunks[0],
    );
    let target = if app.multi_selected.is_empty() {
        app.agents
            .get(app.selected_agent)
            .map(|a| format!("You → {}", a.name))
            .unwrap_or_default()
    } else {
        format!("You → [{}]", app.multi_selected.len())
    };
    f.render_widget(
        Paragraph::new(target)
            .style(Style::default().fg(app.theme.footer).bg(app.theme.surface))
            .alignment(ratatui::layout::Alignment::Center),
        footer_chunks[1],
    );
    f.render_widget(
        Paragraph::new(format!("v{}", env!("CARGO_PKG_VERSION")))
            .style(Style::default().fg(app.theme.footer).bg(app.theme.surface))
            .alignment(ratatui::layout::Alignment::Right),
        footer_chunks[2],
    );

    // ── Help overlay (floats above everything) ───────────────────
    if app.show_help_overlay {
        let area = centered_rect(60, 60, f.size());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(app.theme.accent_primary))
            .title(" Help — ? or Esc to close ")
            .style(Style::default().bg(app.theme.overlay_bg));
        let ver = format!("  v{}", env!("CARGO_PKG_VERSION"));
        let lines: Vec<Line> = [
            "",
            "Keyboard",
            "  Tab         Switch tab (Home → Chat → Monitor → Memory)",
            "  ↑↓          Navigate agents / scroll chat / scroll memory",
            "  Space       Multi-select agent (Home tab)",
            "  Enter       Send message",
            "  Backspace   Clear multi-selection (Home tab)",
            "  Ctrl+C      Quit",
            "",
            "Slash Commands",
            "  /clear                Clear chat history",
            "  /model <name>         Override model for next messages",
            "  /all                  Show all agents in chat",
            "  /evidence claim|verify|fail ...",
            "  /workflow claim <step_id>",
            "  /help                 Show this help",
            "  ?                     Open this overlay",
            "",
            &ver,
        ]
        .iter()
        .map(|s| Line::from(*s))
        .collect();
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .style(Style::default().fg(app.theme.summary)),
            area,
        );
    }
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

fn render_activity(
    activity: &AgentActivity,
    tick: u64,
    theme: &Theme,
) -> (&'static str, Color, String) {
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
                let _ = write!(
                    suffix,
                    "·{}/{}",
                    format_tokens(*cumulative_tokens_in),
                    format_tokens(*cumulative_tokens_out)
                );
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

fn centered_rect(pct_x: u16, pct_y: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let w = area.width * pct_x / 100;
    let h = area.height * pct_y / 100;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    ratatui::layout::Rect::new(x, y, w, h)
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
        let (glyph, _, suffix) =
            render_activity(&AgentActivity::Unknown, 0, &crate::theme::from_env());
        assert_eq!(glyph, "○");
        assert_eq!(suffix, "");
    }

    #[test]
    fn render_activity_idle_with_latency() {
        let (glyph, _, suffix) = render_activity(
            &AgentActivity::Idle {
                last_latency_ms: Some(780),
                cumulative_tokens_in: 0,
                cumulative_tokens_out: 0,
                cumulative_cost_usd: 0.0,
            },
            0,
            &crate::theme::from_env(),
        );
        assert_eq!(glyph, "●");
        assert_eq!(suffix, "780ms");
    }

    #[test]
    fn render_activity_idle_with_tokens() {
        let (glyph, _, suffix) = render_activity(
            &AgentActivity::Idle {
                last_latency_ms: Some(18_600),
                cumulative_tokens_in: 22279,
                cumulative_tokens_out: 284,
                cumulative_cost_usd: 0.0,
            },
            0,
            &crate::theme::from_env(),
        );
        assert_eq!(glyph, "●");
        assert_eq!(suffix, "18.6s·22k/284");
    }

    #[test]
    fn render_activity_idle_no_tokens_when_zero() {
        let (_, _, suffix) = render_activity(
            &AgentActivity::Idle {
                last_latency_ms: Some(100),
                cumulative_tokens_in: 0,
                cumulative_tokens_out: 0,
                cumulative_cost_usd: 0.0,
            },
            0,
            &crate::theme::from_env(),
        );
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
        let (glyph, _, suffix) = render_activity(
            &AgentActivity::Errored {
                last_error: "boom".into(),
            },
            0,
            &crate::theme::from_env(),
        );
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

    // ─── append_chat_message: streaming + handoff bridge ────────────────────

    #[test]
    fn append_chat_message_handoff_pushes_unprefixed() {
        // Synthetic agent "__handoff__" carries pre-formatted lines from
        // the bus→TUI bridge.  on_tick must push them as-is so the leading
        // `⇄` glyph survives (the chat scope filter passes `⇄`-prefixed
        // lines through unconditionally).
        let mut messages: Vec<String> = Vec::new();
        append_chat_message(&mut messages, "__handoff__", "⇄ claude → done");
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].starts_with("⇄"),
            "expected handoff line to start with `⇄`, got: {:?}",
            messages[0]
        );
        assert!(
            !messages[0].contains("__handoff__"),
            "handoff line must NOT contain the synthetic agent name, got: {:?}",
            messages[0]
        );
    }

    #[test]
    fn append_chat_message_text_streams_into_last_when_same_agent() {
        // Regression: streaming chunks from the same agent should merge into
        // a single chat line, not produce multiple "claude: ..." entries.
        let mut messages: Vec<String> = vec!["claude: hel".to_string()];
        append_chat_message(&mut messages, "claude", "lo world");
        assert_eq!(messages, vec!["claude: hello world".to_string()]);
    }

    #[test]
    fn append_chat_message_text_starts_new_line_for_different_agent() {
        let mut messages: Vec<String> = vec!["claude: done".to_string()];
        append_chat_message(&mut messages, "gemini", "hi");
        assert_eq!(
            messages,
            vec!["claude: done".to_string(), "gemini: hi".to_string()]
        );
    }

    // ─── help_lines: SUPPORTED_COMMANDS regression ─────────────────────────

    #[test]
    fn help_lines_documents_every_supported_command() {
        // Every slash command in `SUPPORTED_COMMANDS` MUST appear in /help
        // output.  If you add a new command (match arm in
        // handle_slash_command) without also adding it to SUPPORTED_COMMANDS
        // *and* help_lines(), this test fails — cheap insurance against
        // documentation drift.
        let lines = help_lines();
        let blob = lines.join("\n");
        for cmd in SUPPORTED_COMMANDS {
            assert!(
                blob.contains(cmd),
                "/help output missing documentation for {} — update help_lines() \
                 or remove from SUPPORTED_COMMANDS.  Current /help output:\n{}",
                cmd,
                blob
            );
        }
    }

    #[test]
    fn help_lines_starts_with_slash_commands_header() {
        // Sanity check on the structure — first line should be the slash
        // commands header so users scrolling /help see the commands first.
        let lines = help_lines();
        assert!(
            !lines.is_empty() && lines[0].contains("Slash Commands"),
            "expected first line to be the slash commands header, got: {:?}",
            lines.first()
        );
    }
}
