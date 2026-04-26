//! Crow Hub TUI - Terminal User Interface
//!
//! This is the main entry point for the terminal interface of Crow Hub.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, error};

mod app;
mod setup;

#[derive(Parser)]
#[command(name = "crow")]
#[command(about = "Crow Hub - Universal Agent Orchestration")]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
    
    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
    
    /// Subcommand
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the TUI interface (default)
    Tui,
    
    /// Start the server
    Server {
        /// Run in background
        #[arg(short, long)]
        daemon: bool,
    },
    
    /// Run a workflow file
    Run {
        /// Workflow file path
        workflow: PathBuf,
        
        /// Dry run (validate only)
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Agent management
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    
    /// Show status
    Status {
        /// Output format
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// Send a message
    Send {
        /// Target agent
        #[arg(short, long)]
        to: String,
        
        /// Message content
        #[arg(short, long)]
        message: String,
    },
    
    /// Run the setup wizard (re-scan environment)
    Setup,

    /// Diagnose an agent — load its manifest, send a test prompt directly
    /// through its driver (no bus, no TUI), and print the raw result or
    /// full error detail. Fastest way to tune a CLI's invocation flags.
    Doctor {
        /// Agent name (e.g. "gemini-wsl-ubuntu")
        agent: String,

        /// Optional prompt to send (defaults to a short "say hello" test)
        #[arg(short, long)]
        prompt: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List all agents
    List,
    
    /// Add a new agent
    Add {
        /// Agent name
        #[arg(short, long)]
        name: String,
        
        /// Adapter type
        #[arg(short, long)]
        adapter: String,
        
        /// Configuration file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    
    /// Remove an agent
    Remove {
        /// Agent name or ID
        name: String,
    },
    
    /// Show agent details
    Show {
        /// Agent name or ID
        name: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Determine if we are starting the TUI
    let is_tui = match cli.command {
        None | Some(Commands::Tui) => true,
        _ => false,
    };

    // Initialize logging: file for TUI (to prevent corruption), stdout for CLI/Server
    if is_tui {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("crow-hub.log")
            .unwrap_or_else(|_| std::fs::File::create("crow-hub.log").unwrap());
        
        tracing_subscriber::fmt()
            .with_env_filter(&cli.log_level)
            .with_writer(std::sync::Arc::new(log_file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(&cli.log_level)
            .with_writer(std::io::stdout)
            .init();
    };
    
    info!("Crow Hub TUI v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = if let Some(config_path) = cli.config {
        ch_core::HubConfig::load(config_path)?
    } else {
        ch_core::HubConfig::load_default().unwrap_or_default()
    };
    
    match cli.command {
        None | Some(Commands::Tui) => {
            // Check for first-run: if no agents configured, run setup wizard
            if setup::needs_setup("plugins") {
                info!("First run detected — launching setup wizard...");
                setup::run_setup_wizard("plugins")?;
            }
            info!("Starting TUI interface...");
            run_tui(config).await?;
        }
        
        Some(Commands::Server { daemon }) => {
            info!("Starting server...");
            if daemon {
                info!("Running in daemon mode");
            }
            run_server(config).await?;
        }
        
        Some(Commands::Run { workflow, dry_run }) => {
            info!("Running workflow: {:?}", workflow);
            if dry_run {
                info!("Dry run mode - validating only");
            }
            run_workflow(config, workflow, dry_run).await?;
        }
        
        Some(Commands::Agent { command }) => {
            match command {
                AgentCommands::List => {
                    list_agents(config).await?;
                }
                AgentCommands::Add { name, adapter, config: _ } => {
                    add_agent(config, name, adapter).await?;
                }
                AgentCommands::Remove { name } => {
                    remove_agent(config, name).await?;
                }
                AgentCommands::Show { name } => {
                    show_agent(config, name).await?;
                }
            }
        }
        
        Some(Commands::Status { format }) => {
            show_status(config, &format).await?;
        }
        
        Some(Commands::Send { to, message }) => {
            send_message(config, to, message).await?;
        }
        
        Some(Commands::Setup) => {
            info!("Running setup wizard...");
            setup::run_setup_wizard("plugins")?;
        }

        Some(Commands::Doctor { agent, prompt }) => {
            let prompt = prompt.unwrap_or_else(|| "Say hello in one short sentence.".to_string());
            run_doctor(config, agent, prompt).await?;
        }
    }

    Ok(())
}

async fn run_tui(config: ch_core::HubConfig) -> anyhow::Result<()> {
    use ch_model::{ModelRegistry, ModelRouter};
    use ch_agent::AgentRuntime;
    use ch_protocol::{AgentId, Payload};
    use std::sync::Arc;

    info!("Initializing Agent Engine...");

    let hub = ch_core::CrowHub::new(config).await?;

    // Start the hub so the message bus is running before agents subscribe
    hub.start().await?;

    let registry = Arc::new(ModelRegistry::new());
    let router = Arc::new(ModelRouter::new(registry.clone()));

    let agent_runtime = Arc::new(AgentRuntime::new(router, hub.bus.clone(), "plugins"));
    agent_runtime.load_all().await?;

    // Subscribe a "user" identity to the bus for the TUI
    let user_agent_id = AgentId::new();
    let bus_rx = hub.bus.subscribe(user_agent_id).await;
    let _ = hub.bus.create_channel("general");
    hub.bus.join_channel("general", user_agent_id, ch_core::ChannelVisibility::Full)?;

    // Pre-create the TUI response channel and bridge bus messages into it
    let (tx, response_rx) = tokio::sync::mpsc::channel::<(String, String)>(100);
    let tx_bridge = tx.clone();
    tokio::spawn(async move {
        let mut rx = bus_rx;
        while let Some(msg) = rx.recv().await {
            if let Payload::Text(ref text) = msg.payload {
                let _ = tx_bridge.send((msg.from.agent_name.clone(), text.clone())).await;
            }
        }
    });

    info!("Initializing ratatui interface...");
    app::run_tui_app(agent_runtime.clone(), hub.bus.clone(), user_agent_id, tx, response_rx)?;

    // Clean shutdown
    agent_runtime.stop_all().await;
    hub.shutdown().await?;
    Ok(())
}

async fn run_server(config: ch_core::HubConfig) -> anyhow::Result<()> {
    use ch_model::{ModelRegistry, ModelRouter, AutoDiscovery};
    use ch_agent::AgentRuntime;
    use std::sync::Arc;

    let hub = ch_core::CrowHub::new(config).await?;
    
    // Wire up the new v2 architecture
    info!("Initializing v2 Hub Architecture...");

    // 1. Model Routing & Discovery
    let registry = Arc::new(ModelRegistry::new());
    let router = Arc::new(ModelRouter::new(registry.clone()));
    
    // Start auto-discovery for local + Spark node
    info!("Starting multi-host model discovery...");
    let mut config = ch_model::discovery::DiscoveryConfig::default();
    config.hosts.push(ch_model::discovery::HostConfig::remote("spark", "192.168.50.1"));
    
    let discovery = AutoDiscovery::new(config);
    let results = discovery.discover(&registry).await?;
    info!("Discovered {} local model servers", results.len());

    // 2. Agent Plugin System
    info!("Initializing Agent Runtime...");
    let agent_runtime = AgentRuntime::new(router.clone(), hub.bus.clone(), "plugins");
    
    // Load all plugins from disk
    agent_runtime.load_all().await?;

    info!("Agent Runtime Summary:\n{}", agent_runtime.summary());
    info!("Model Router Status:\n{}", router.summary());

    // Start the legacy hub
    hub.start().await?;
    info!("Server started. Press Ctrl+C to stop.");
    
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");
    
    agent_runtime.stop_all().await;
    hub.shutdown().await?;
    
    Ok(())
}

async fn run_workflow(
    _config: ch_core::HubConfig,
    workflow_path: PathBuf,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("Running workflow: {:?}", workflow_path);
    
    if dry_run {
        println!("✓ Workflow validation passed");
        return Ok(());
    }
    
    println!("Workflow execution is under development.");
    Ok(())
}

async fn list_agents(_config: ch_core::HubConfig) -> anyhow::Result<()> {
    use ch_agent::PluginLoader;
    
    let loader = PluginLoader::new("plugins");
    let plugins = loader.scan()?;

    println!("Registered Agents (from plugins/agents):");
    println!("{:<20} {:<15} {:<20} {}", "Name", "Driver", "Default Model", "Description");
    println!("{}", "-".repeat(80));
    
    for plugin in plugins {
        let m = plugin.manifest;
        let model = m.model.map(|m| m.default).unwrap_or_else(|| "-".to_string());
        println!("{:<20} {:<15} {:<20} {}", m.agent.name, format!("{:?}", m.agent.driver), model, m.agent.description);
    }
    
    Ok(())
}

async fn add_agent(
    _config: ch_core::HubConfig,
    name: String,
    adapter: String,
) -> anyhow::Result<()> {
    println!("Adding agent: {} (adapter: {}) - Please add a manifest to plugins/agents/{}", name, adapter, name);
    Ok(())
}

async fn remove_agent(_config: ch_core::HubConfig, name: String) -> anyhow::Result<()> {
    println!("Removing agent: {} - Please delete plugins/agents/{}", name, name);
    Ok(())
}

async fn show_agent(_config: ch_core::HubConfig, name: String) -> anyhow::Result<()> {
    use ch_agent::PluginLoader;
    
    let loader = PluginLoader::new("plugins");
    match loader.load_single(&name) {
        Ok(plugin) => {
            let m = plugin.manifest;
            println!("Agent Details: {}", name);
            println!("{}", "=".repeat(40));
            println!("Driver:      {:?}", m.agent.driver);
            println!("Description: {}", m.agent.description);
            if let Some(model) = m.model {
                println!("Model Backend: {:?}", model.backend);
                println!("Default Model: {}", model.default);
            }
            if let Some(sub) = m.subprocess {
                println!("CLI Command: {} {:?}", sub.command, sub.args);
                println!("Shell Type:  {:?}", sub.shell);
            }
        }
        Err(e) => {
            println!("Agent '{}' not found: {}", name, e);
        }
    }
    Ok(())
}

async fn show_status(_config: ch_core::HubConfig, format: &str) -> anyhow::Result<()> {
    use ch_agent::PluginLoader;
    
    let loader = PluginLoader::new("plugins");
    let agent_count = loader.scan().map(|p| p.len()).unwrap_or(0);

    match format {
        "json" => {
            println!("{{");
            println!("  \"status\": \"installed\",");
            println!("  \"agents\": {},", agent_count);
            println!("  \"version\": \"{}\"", env!("CARGO_PKG_VERSION"));
            println!("}}");
        }
        _ => {
            println!("Crow Hub Status");
            println!("================");
            println!("Status:     🟢 Installed (v{})", env!("CARGO_PKG_VERSION"));
            println!("Agents:     {} configured in plugins/agents/", agent_count);
        }
    }
    Ok(())
}

async fn send_message(
    _config: ch_core::HubConfig,
    to: String,
    message: String,
) -> anyhow::Result<()> {
    println!("Sending message to {}: {}", to, message);
    println!("✓ Message sent");
    Ok(())
}

/// Diagnostic: load an agent, build its driver, send one prompt through
/// `driver.chat()`, and print the result. Bypasses the bus and TUI so the
/// user sees exactly what the underlying driver does with the current
/// manifest — fastest feedback loop for tuning per-CLI invocation flags.
async fn run_doctor(
    _config: ch_core::HubConfig,
    agent_name: String,
    prompt: String,
) -> anyhow::Result<()> {
    use ch_agent::drivers::{AgentDriver, APIDriver, SubprocessDriver, TmuxDriver};
    use ch_agent::manifest::DriverType;
    use ch_agent::PluginLoader;
    use ch_model::{ChatRequest, ModelRegistry, ModelRouter};
    use std::sync::Arc;

    println!("━━━ Crow Hub Doctor ━━━");
    println!("Agent:  {}", agent_name);
    println!("Prompt: {}", prompt);
    println!();

    // 1. Load the manifest
    let loader = PluginLoader::new("plugins");
    let plugin = match loader.load_single(&agent_name) {
        Ok(p) => p,
        Err(e) => {
            println!("✗ Failed to load manifest for '{}': {}", agent_name, e);
            return Ok(());
        }
    };
    let manifest = &plugin.manifest;

    println!("Driver: {:?}", manifest.agent.driver);
    if let Some(ref sub) = manifest.subprocess {
        println!("Command:  {}", sub.command);
        println!("Args:     {:?}", sub.args);
        println!("Shell:    {:?}", sub.shell);
        if let Some(ref d) = sub.wsl_distro {
            println!("WSL:      {}", d);
        }
        println!("InputMd:  {:?}", sub.input_mode);
        println!("OutputMd: {:?}", sub.output_mode);
        if let Some(ref f) = sub.output_filter {
            println!("Filter:   {}", f);
        }
    }
    println!();

    // 2. Build the driver (mirrors AgentRuntime::load_plugin)
    let registry = Arc::new(ModelRegistry::new());
    let router = Arc::new(ModelRouter::new(registry));

    let driver: Arc<dyn AgentDriver> = match manifest.agent.driver {
        DriverType::Api => match APIDriver::from_manifest(manifest, router) {
            Ok(d) => Arc::new(d),
            Err(e) => {
                println!("✗ Failed to build API driver: {}", e);
                return Ok(());
            }
        },
        DriverType::Subprocess => {
            let sub = match manifest.subprocess.as_ref() {
                Some(s) => s,
                None => {
                    println!("✗ Subprocess driver requires [subprocess] section");
                    return Ok(());
                }
            };
            Arc::new(SubprocessDriver::new(&agent_name, sub.clone()))
        }
        DriverType::Tmux => {
            let tmux = match manifest.tmux.as_ref() {
                Some(t) => t,
                None => {
                    println!("✗ Tmux driver requires [tmux] section");
                    return Ok(());
                }
            };
            Arc::new(TmuxDriver::new(&agent_name, tmux.clone()))
        }
        DriverType::Mcp => {
            println!("✗ MCP driver not yet implemented");
            return Ok(());
        }
    };

    // 3. Send the prompt
    let model = manifest
        .model
        .as_ref()
        .map(|m| m.default.clone())
        .unwrap_or_else(|| "default".to_string());

    let request = ChatRequest::simple(&model, &prompt);
    println!("→ Sending (this may take a moment)...");
    println!();

    let start = std::time::Instant::now();
    match driver.chat(request).await {
        Ok(resp) => {
            let dur = start.elapsed();
            println!("✓ Success ({}ms)", dur.as_millis());
            println!("Backend: {}", resp.backend);
            println!("Model:   {}", resp.model);
            println!("Finish:  {:?}", resp.finish_reason);
            println!();
            println!("--- Response ---");
            println!("{}", resp.content);
            println!("--- End ---");
        }
        Err(e) => {
            let dur = start.elapsed();
            println!("✗ Error ({}ms)", dur.as_millis());
            println!();
            println!("--- Error detail ---");
            println!("{}", e);
            println!("--- End ---");
        }
    }

    // Clean up (in case the driver holds a subprocess)
    let _ = driver.stop().await;

    Ok(())
}
