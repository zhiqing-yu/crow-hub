//! Setup Wizard
//!
//! First-run interactive setup that scans the environment for
//! available agents and lets the user choose which ones to enable.

use ch_agent::scanner::{EnvironmentScanner, ScanEnvironment, ScanResults};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::Stylize,
    terminal::{self, ClearType},
};
use std::io::{self, Write};

/// Check if the first-run setup wizard should be shown
pub fn needs_setup(plugins_dir: &str) -> bool {
    let agents_dir = std::path::Path::new(plugins_dir).join("agents");
    if !agents_dir.exists() {
        return true;
    }
    // Check if there are any agent.toml files
    match std::fs::read_dir(&agents_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let manifest = entry.path().join("agent.toml");
                    if manifest.exists() {
                        return false; // At least one agent exists
                    }
                }
            }
            true // Directory exists but no agents
        }
        Err(_) => true,
    }
}



/// Run the interactive setup wizard. Returns Ok(true) if setup completed.
pub fn run_setup_wizard(plugins_dir: &str) -> anyhow::Result<bool> {
    let mut stdout = io::stdout();

    // ── Step 1: Collect Environments ──
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let mut scan_targets = Vec::new();

    // 1a & 1b: Local Environments (Native + WSL)
    let mut local_envs = Vec::new();
    local_envs.push(ScanEnvironment::Native(EnvironmentScanner::detect_native_os()));
    
    for d in EnvironmentScanner::detect_wsl_distros() {
        local_envs.push(ScanEnvironment::Wsl(d));
    }
    
    let picked_local = run_local_env_picker(&mut stdout, local_envs)?;
    for env in picked_local {
        scan_targets.push(env);
    }

    // 1c. Collect SSH
    let ssh_hosts = run_ssh_picker(&mut stdout, &scan_targets)?;
    for (host, user) in ssh_hosts {
         scan_targets.push(ScanEnvironment::Ssh { host, user });
    }

    // ── Step 2: Confirm & Scan ──
    draw_scanning_screen(&mut stdout, &scan_targets)?;

    let scanner = EnvironmentScanner::new(scan_targets);

    terminal::disable_raw_mode()?;
    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

    println!("\n🔍 Scanning chosen environments...\n");
    let results = scanner.scan();
    println!(
        "\n✓ Found {} CLI agents\n",
        results.agents.len()
    );

    if results.agents.is_empty() {
        println!("No agents found.");
        println!("You can add custom agents later with: crow setup");
        println!("\nPress Enter to continue...");
        let _ = read_line();
        return Ok(true);
    }

    // ── Step 3: Selection ──
    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let selected = run_selection_screen(&mut stdout, results)?;

    terminal::disable_raw_mode()?;
    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

    // ── Step 4: Generate manifests ──
    let plugins_path = std::path::Path::new(plugins_dir);
    let mut count = 0;

    for agent in &selected.agents {
        if agent.selected {
            agent.write_manifest(plugins_path)?;
            count += 1;
        }
    }



    println!("\n✓ Created {} agent configuration(s) in plugins/agents/", count);
    println!("\nPress Enter to launch Crow Hub...");
    let _ = read_line();

    Ok(count > 0)
}

/// Helper to pick local environments (Native + WSL)
fn run_local_env_picker(
    stdout: &mut io::Stdout,
    envs: Vec<ScanEnvironment>,
) -> anyhow::Result<Vec<ScanEnvironment>> {
    let mut selected = vec![true; envs.len()]; // Default to selecting all local envs
    let mut cursor = 0;

    loop {
        execute!(stdout, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        writeln!(stdout, "{}", "╔══════════════════════════════════════════════════════════════╗".cyan())?;
        writeln!(stdout, "{}", "║             🌐 Welcome to Crow Hub Setup                   ║".cyan())?;
        writeln!(stdout, "{}", "╚══════════════════════════════════════════════════════════════╝".cyan())?;
        writeln!(stdout, "\n  We detected the following local environments on this system.")?;
        writeln!(stdout, "  Which ones do you want to safely scan for AI agents?\n")?;

        for (i, env) in envs.iter().enumerate() {
            let checkbox = if selected[i] { "[x]" } else { "[ ]" };
            let line = format!("  {} {}", checkbox, env);
            if i == cursor {
                writeln!(stdout, "{}", line.on_dark_grey())?;
            } else {
                writeln!(stdout, "{}", line)?;
            }
        }

        writeln!(stdout, "\n  {} Toggle   {} Continue", "[Space]".green(), "[Enter]".cyan())?;
        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press { continue; }
            match key.code {
                KeyCode::Up => if cursor > 0 { cursor -= 1; },
                KeyCode::Down => if cursor + 1 < envs.len() { cursor += 1; },
                KeyCode::Char(' ') => selected[cursor] = !selected[cursor],
                KeyCode::Enter => break,
                KeyCode::Esc => return Ok(Vec::new()),
                _ => {}
            }
        }
    }

    let mut chosen = Vec::new();
    for (i, d) in envs.into_iter().enumerate() {
        if selected[i] { chosen.push(d); }
    }
    Ok(chosen)
}

/// Collect SSH hosts from user input
fn run_ssh_picker(
    stdout: &mut io::Stdout,
    current_targets: &[ScanEnvironment],
) -> anyhow::Result<Vec<(String, String)>> {
    let mut hosts: Vec<(String, String)> = Vec::new();

    loop {
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        writeln!(stdout, "{}", "╔══════════════════════════════════════════════════════════════╗".cyan())?;
        writeln!(stdout, "{}", "║             🌐 Welcome to Crow Hub Setup                   ║".cyan())?;
        writeln!(stdout, "{}", "╚══════════════════════════════════════════════════════════════╝".cyan())?;
        writeln!(stdout)?;
        writeln!(stdout, "  Selected Environments:")?;
        for t in current_targets {
            match t {
                ScanEnvironment::Native(os) => writeln!(stdout, "    • {} Native", os)?,
                ScanEnvironment::Wsl(d) => writeln!(stdout, "    • WSL: {}", d)?,
                ScanEnvironment::Ssh { host, user } => writeln!(stdout, "    • SSH: {}@{}", user, host)?,
            }
        }
        for (h, u) in &hosts {
             writeln!(stdout, "    • SSH: {}@{}", u, h)?;
        }
        
        writeln!(stdout)?;
        writeln!(stdout, "  Do you want to add any SSH-connected remote machines?")?;
        writeln!(stdout)?;
        writeln!(stdout, "  {} Add SSH host", "[a]".green())?;
        writeln!(stdout, "  {} Continue to scan", "[Enter]".cyan())?;
        writeln!(stdout, "  {} Quit Setup", "[Esc]".red())?;
        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press { continue; }
            match key.code {
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    terminal::disable_raw_mode()?;
                    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

                    print!("\n  SSH Host IP/hostname: ");
                    io::stdout().flush()?;
                    let host = read_line()?;

                    print!("  SSH Username: ");
                    io::stdout().flush()?;
                    let user = read_line()?;

                    if !host.trim().is_empty() && !user.trim().is_empty() {
                        hosts.push((host.trim().to_string(), user.trim().to_string()));
                    }

                    terminal::enable_raw_mode()?;
                    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
                }
                KeyCode::Enter => break,
                KeyCode::Esc => {
                    std::process::exit(0);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    }

    Ok(hosts)
}

/// Draw a "scanning..." screen confirms exactly what is scanned
fn draw_scanning_screen(
    stdout: &mut io::Stdout,
    targets: &[ScanEnvironment]
) -> anyhow::Result<()> {
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 2)
    )?;
    writeln!(stdout, "  Ready to scan {} environments for AI agents:", targets.len().to_string().cyan())?;
    for t in targets {
        writeln!(stdout, "   • {}", t)?;
    }
    writeln!(stdout, "\n  This may take a moment...")?;
    stdout.flush()?;
    Ok(())
}

/// Interactive agent selection screen
fn run_selection_screen(
    stdout: &mut io::Stdout,
    mut results: ScanResults,
) -> anyhow::Result<ScanResults> {
    let total_items = results.agents.len();
    let mut cursor_pos: usize = 0;

    loop {
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        writeln!(stdout, "{}",
            "  Select agents to enable (Space=toggle, Enter=confirm, a=select all)"
                .cyan()
        )?;
        writeln!(stdout)?;

        // Group by environment
        let mut row: usize = 0;

        // -- CLI Agents --
        if !results.agents.is_empty() {
            // Group agents by environment
            let mut env_groups: Vec<(String, Vec<usize>)> = Vec::new();
            for (i, agent) in results.agents.iter().enumerate() {
                let env_key = format!("{}", agent.environment);
                if let Some(group) = env_groups.iter_mut().find(|(k, _)| k == &env_key) {
                    group.1.push(i);
                } else {
                    env_groups.push((env_key, vec![i]));
                }
            }

            for (env_label, indices) in &env_groups {
                writeln!(stdout, "  ┌─ {} ─────────────────────────────────────┐",
                    env_label.clone().green()
                )?;

                for &idx in indices {
                    let agent = &results.agents[idx];
                    let checkbox = if agent.selected { "[x]" } else { "[ ]" };
                    let line = format!(
                        "  │ {} {:<16} CLI  {}",
                        checkbox, agent.display_name, agent.binary
                    );

                    if row == cursor_pos {
                        writeln!(stdout, "{}", line.on_dark_grey())?;
                    } else {
                        writeln!(stdout, "{}", line)?;
                    }
                    row += 1;
                }

                writeln!(stdout, "  └─────────────────────────────────────────────┘")?;
            }
        }



        let selected_count = results.agents.iter().filter(|a| a.selected).count();

        writeln!(stdout)?;
        writeln!(stdout, "  {} selected  |  {} Toggle  {} Confirm  {} All  {} Quit",
            selected_count,
            "[Space]".green(),
            "[Enter]".cyan(),
            "[a]".yellow(),
            "[Esc]".red(),
        )?;
        stdout.flush()?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up => {
                    if cursor_pos > 0 {
                        cursor_pos -= 1;
                    }
                }
                KeyCode::Down => {
                    if cursor_pos + 1 < total_items {
                        cursor_pos += 1;
                    }
                }
                KeyCode::Char(' ') => {
                    // Toggle selection
                    let agent_count = results.agents.len();
                    if cursor_pos < agent_count {
                        results.agents[cursor_pos].selected = !results.agents[cursor_pos].selected;
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    // Select all
                    let all_selected = results.agents.iter().all(|a| a.selected);
                    let new_state = !all_selected;
                    for a in &mut results.agents {
                        a.selected = new_state;
                    }

                }
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Esc => {
                    // Deselect all and break
                    for a in &mut results.agents {
                        a.selected = false;
                    }

                    break;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    for a in &mut results.agents {
                        a.selected = false;
                    }

                    break;
                }
                _ => {}
            }
        }
    }

    Ok(results)
}

/// Read a line from stdin (non-raw mode)
fn read_line() -> anyhow::Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
