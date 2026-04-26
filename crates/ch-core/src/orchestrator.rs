//! Orchestrator
//!
//! Coordinates agent workflows and task execution

use ch_protocol::{AgentMessage, AgentId, Workflow, WorkflowStep, TaskSpec, Payload, MessageType};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

use crate::{MessageBus, AgentRegistry, SessionManager, Result, CoreError};

/// Orchestrator state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorState {
    Idle,
    Running,
    Paused,
    ShuttingDown,
}

/// Task execution engine
pub struct Orchestrator {
    /// Message bus reference
    bus: Arc<MessageBus>,
    /// Agent registry reference
    registry: Arc<AgentRegistry>,
    /// Session manager reference
    sessions: Arc<SessionManager>,
    /// Current state
    state: Arc<RwLock<OrchestratorState>>,
    /// Command channel
    command_tx: mpsc::Sender<OrchestratorCommand>,
    command_rx: Arc<RwLock<mpsc::Receiver<OrchestratorCommand>>>,
}

/// Commands for the orchestrator
#[derive(Debug)]
pub enum OrchestratorCommand {
    Start,
    Pause,
    Resume,
    Shutdown,
    ExecuteWorkflow(Workflow),
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(
        bus: Arc<MessageBus>,
        registry: Arc<AgentRegistry>,
        sessions: Arc<SessionManager>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(100);
        
        Self {
            bus,
            registry,
            sessions,
            state: Arc::new(RwLock::new(OrchestratorState::Idle)),
            command_tx,
            command_rx: Arc::new(RwLock::new(command_rx)),
        }
    }
    
    /// Get command sender
    pub fn command_sender(&self) -> mpsc::Sender<OrchestratorCommand> {
        self.command_tx.clone()
    }
    
    /// Start the orchestrator
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = OrchestratorState::Running;
        
        // Start command processing loop
        let command_rx = self.command_rx.clone();
        let bus = self.bus.clone();
        let registry = self.registry.clone();
        let sessions = self.sessions.clone();
        let state = self.state.clone();
        
        tokio::spawn(async move {
            let mut rx = command_rx.write().await;
            
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    OrchestratorCommand::Start => {
                        info!("Orchestrator starting");
                    }
                    OrchestratorCommand::Pause => {
                        let mut s = state.write().await;
                        *s = OrchestratorState::Paused;
                        info!("Orchestrator paused");
                    }
                    OrchestratorCommand::Resume => {
                        let mut s = state.write().await;
                        *s = OrchestratorState::Running;
                        info!("Orchestrator resumed");
                    }
                    OrchestratorCommand::Shutdown => {
                        let mut s = state.write().await;
                        *s = OrchestratorState::ShuttingDown;
                        info!("Orchestrator shutting down");
                        break;
                    }
                    OrchestratorCommand::ExecuteWorkflow(workflow) => {
                        if let Err(e) = Self::execute_workflow(
                            &bus, &registry, &sessions, workflow
                        ).await {
                            error!("Workflow execution failed: {}", e);
                        }
                    }
                }
            }
        });
        
        info!("Orchestrator started");
        Ok(())
    }
    
    /// Shutdown the orchestrator
    pub async fn shutdown(&self) -> Result<()> {
        self.command_tx.send(OrchestratorCommand::Shutdown)
            .await
            .map_err(|e| CoreError::Orchestration(e.to_string()))?;
        
        let mut state = self.state.write().await;
        *state = OrchestratorState::Idle;
        
        Ok(())
    }
    
    /// Execute a workflow
    async fn execute_workflow(
        bus: &MessageBus,
        registry: &AgentRegistry,
        sessions: &SessionManager,
        workflow: Workflow,
    ) -> Result<()> {
        info!("Executing workflow: {}", workflow.name);
        
        // Build dependency graph
        let mut completed_steps = std::collections::HashSet::new();
        let mut pending_steps: Vec<&WorkflowStep> = workflow.steps.iter().collect();
        
        while !pending_steps.is_empty() {
            // Find steps that can be executed (dependencies satisfied)
            let ready_steps: Vec<&WorkflowStep> = pending_steps.iter()
                .filter(|step| {
                    step.depends_on.iter().all(|dep| completed_steps.contains(dep))
                })
                .copied()
                .collect();
            
            if ready_steps.is_empty() && !pending_steps.is_empty() {
                return Err(CoreError::Orchestration(
                    "Circular dependency detected in workflow".to_string()
                ));
            }
            
            // Execute ready steps in parallel
            let mut handles = Vec::new();
            
            for step in ready_steps {
                let _bus = bus.clone();
                let step_id = step.step_id.clone();
                let step = step.clone();
                
                let handle = tokio::spawn(async move {
                    debug!("Executing step: {}", step.name);
                    
                    // Find agent
                    let _agent_id = AgentId::default(); // Parse from step.agent_id
                    
                    // Create task message
                    let _task = TaskSpec {
                        task_id: step.step_id.clone(),
                        description: step.action.clone(),
                        requirements: vec![],
                        deadline: None,
                        dependencies: step.depends_on.clone(),
                        metadata: step.inputs.iter()
                            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                            .collect(),
                    };
                    
                    // Send task to agent
                    // ...
                    
                    Ok::<(), CoreError>(())
                });
                
                handles.push(handle);
                completed_steps.insert(step_id);
            }
            
            // Wait for all parallel steps
            for handle in handles {
                if let Err(e) = handle.await {
                    error!("Step execution failed: {}", e);
                }
            }
            
            // Remove completed steps from pending
            pending_steps.retain(|step| !completed_steps.contains(&step.step_id));
        }
        
        info!("Workflow {} completed", workflow.name);
        Ok(())
    }
    
    /// Get current state
    pub async fn state(&self) -> OrchestratorState {
        *self.state.read().await
    }
    
    /// Check if running
    pub async fn is_running(&self) -> bool {
        matches!(self.state().await, OrchestratorState::Running)
    }
}
