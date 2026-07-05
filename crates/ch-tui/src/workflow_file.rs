//! Parses and validates the YAML workflow file format used by `crow run`.
//!
//! This is a distinct shape from `ch_protocol::types::Workflow` (which is
//! the *internal* representation used for bus messages) — the on-disk file
//! wraps everything under a top-level `workflow:` key and uses `agent`
//! (singular) rather than `agent_id`, matching what's documented in the
//! README and `examples/simple-workflow.yaml`. Nothing previously parsed
//! this file at all: `crow run` unconditionally printed "validation
//! passed" without reading it.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Deserialize)]
pub struct WorkflowFile {
    pub workflow: WorkflowDef,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agents: Vec<AgentDef>,
    pub steps: Vec<StepDef>,
}

#[derive(Debug, Deserialize)]
pub struct AgentDef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StepDef {
    pub id: String,
    pub name: String,
    pub agent: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("duplicate step id: {0}")]
    DuplicateStepId(String),
    #[error("step '{step}' depends_on unknown step '{dep}'")]
    UnknownDependency { step: String, dep: String },
    #[error("step '{step}' references unknown agent '{agent}'")]
    UnknownAgent { step: String, agent: String },
    #[error("workflow has no steps")]
    NoSteps,
    #[error("dependency cycle detected involving step '{0}'")]
    Cycle(String),
}

/// Parse a workflow YAML file's contents.
pub fn parse(yaml: &str) -> Result<WorkflowFile, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// Validate structural integrity and return steps in dependency order
/// (topological sort via Kahn's algorithm) so callers can execute/print
/// them in a safe sequence.
pub fn validate_and_order(def: &WorkflowDef) -> Result<Vec<String>, ValidationError> {
    if def.steps.is_empty() {
        return Err(ValidationError::NoSteps);
    }

    let mut seen = HashSet::new();
    for step in &def.steps {
        if !seen.insert(step.id.as_str()) {
            return Err(ValidationError::DuplicateStepId(step.id.clone()));
        }
    }

    // Agents are optional in the file (a step may reference an agent not
    // declared up top, e.g. one already known to crow-hub) — only validate
    // when the `agents:` block is non-empty, since an empty block means
    // "not specified here."
    if !def.agents.is_empty() {
        let agent_ids: HashSet<&str> = def.agents.iter().map(|a| a.id.as_str()).collect();
        for step in &def.steps {
            if !agent_ids.contains(step.agent.as_str()) {
                return Err(ValidationError::UnknownAgent {
                    step: step.id.clone(),
                    agent: step.agent.clone(),
                });
            }
        }
    }

    let mut in_degree: HashMap<&str, usize> = def.steps.iter().map(|s| (s.id.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for step in &def.steps {
        for dep in &step.depends_on {
            if !seen.contains(dep.as_str()) {
                return Err(ValidationError::UnknownDependency {
                    step: step.id.clone(),
                    dep: dep.clone(),
                });
            }
            *in_degree.get_mut(step.id.as_str()).unwrap() += 1;
            dependents.entry(dep.as_str()).or_default().push(step.id.as_str());
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort(); // deterministic order among independent steps
    let mut order = Vec::with_capacity(def.steps.len());

    while let Some(id) = queue.pop() {
        order.push(id.to_string());
        if let Some(deps) = dependents.get(id) {
            let mut newly_ready = Vec::new();
            for &dep_id in deps {
                let deg = in_degree.get_mut(dep_id).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    newly_ready.push(dep_id);
                }
            }
            newly_ready.sort();
            queue.extend(newly_ready);
        }
    }

    if order.len() != def.steps.len() {
        // Whatever's left has nonzero in-degree — part of a cycle.
        let stuck = def
            .steps
            .iter()
            .find(|s| !order.contains(&s.id))
            .map(|s| s.id.clone())
            .unwrap_or_default();
        return Err(ValidationError::Cycle(stuck));
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = r#"
workflow:
  name: "Simple Chat Workflow"
  agents:
    - id: "claude"
      adapter: "claude"
    - id: "kimi"
      adapter: "kimi"
  steps:
    - id: "step-1"
      name: "Initial Analysis"
      agent: "claude"
    - id: "step-2"
      name: "Deep Research"
      agent: "kimi"
      depends_on:
        - "step-1"
    - id: "step-3"
      name: "Final Summary"
      agent: "claude"
      depends_on:
        - "step-2"
"#;

    #[test]
    fn parses_documented_example_shape() {
        let file = parse(VALID_YAML).expect("should parse");
        assert_eq!(file.workflow.name, "Simple Chat Workflow");
        assert_eq!(file.workflow.steps.len(), 3);
    }

    #[test]
    fn validate_and_order_respects_depends_on() {
        let file = parse(VALID_YAML).unwrap();
        let order = validate_and_order(&file.workflow).unwrap();
        assert_eq!(order, vec!["step-1", "step-2", "step-3"]);
    }

    #[test]
    fn rejects_duplicate_step_id() {
        let yaml = r#"
workflow:
  name: "dup"
  steps:
    - id: "a"
      name: "A"
      agent: "x"
    - id: "a"
      name: "A again"
      agent: "x"
"#;
        let file = parse(yaml).unwrap();
        let err = validate_and_order(&file.workflow).unwrap_err();
        assert_eq!(err, ValidationError::DuplicateStepId("a".to_string()));
    }

    #[test]
    fn rejects_unknown_dependency() {
        let yaml = r#"
workflow:
  name: "bad-dep"
  steps:
    - id: "a"
      name: "A"
      agent: "x"
      depends_on: ["missing"]
"#;
        let file = parse(yaml).unwrap();
        let err = validate_and_order(&file.workflow).unwrap_err();
        assert_eq!(
            err,
            ValidationError::UnknownDependency {
                step: "a".to_string(),
                dep: "missing".to_string()
            }
        );
    }

    #[test]
    fn rejects_unknown_agent_when_agents_declared() {
        let yaml = r#"
workflow:
  name: "bad-agent"
  agents:
    - id: "claude"
  steps:
    - id: "a"
      name: "A"
      agent: "ghost"
"#;
        let file = parse(yaml).unwrap();
        let err = validate_and_order(&file.workflow).unwrap_err();
        assert_eq!(
            err,
            ValidationError::UnknownAgent {
                step: "a".to_string(),
                agent: "ghost".to_string()
            }
        );
    }

    #[test]
    fn allows_unknown_agent_when_no_agents_declared() {
        // If the file doesn't declare an `agents:` block, we can't validate
        // agent references against it — crow-hub's own agent registry is
        // the source of truth in that case.
        let yaml = r#"
workflow:
  name: "no-agents-block"
  steps:
    - id: "a"
      name: "A"
      agent: "whatever"
"#;
        let file = parse(yaml).unwrap();
        assert!(validate_and_order(&file.workflow).is_ok());
    }

    #[test]
    fn rejects_cycle() {
        let yaml = r#"
workflow:
  name: "cycle"
  steps:
    - id: "a"
      name: "A"
      agent: "x"
      depends_on: ["b"]
    - id: "b"
      name: "B"
      agent: "x"
      depends_on: ["a"]
"#;
        let file = parse(yaml).unwrap();
        let err = validate_and_order(&file.workflow).unwrap_err();
        assert!(matches!(err, ValidationError::Cycle(_)));
    }

    #[test]
    fn rejects_empty_steps() {
        let yaml = r#"
workflow:
  name: "empty"
  steps: []
"#;
        let file = parse(yaml).unwrap();
        let err = validate_and_order(&file.workflow).unwrap_err();
        assert_eq!(err, ValidationError::NoSteps);
    }
}
