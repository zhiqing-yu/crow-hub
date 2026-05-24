//! Autonomous evidence verifier — polls store and applies rules to pending rows.

use crate::{EvidenceRow, EvidenceStore};
use async_trait::async_trait;
use ch_core::bus::MessageBus;
use ch_protocol::{AgentAddress, AgentId, AgentMessage, MessageType, Payload, EvidenceVerifyMsg};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub enum RuleOutcome {
    Verify { witness: Option<String> },
    Fail { reason: String },
    Skip,
}

#[async_trait]
pub trait VerifierRule: Send + Sync {
    fn name(&self) -> &'static str;
    async fn evaluate(&self, row: &EvidenceRow) -> RuleOutcome;
}

// ── KeywordRule ──────────────────────────────────────────────

pub struct KeywordRule;

#[async_trait]
impl VerifierRule for KeywordRule {
    fn name(&self) -> &'static str { "keyword" }
    async fn evaluate(&self, row: &EvidenceRow) -> RuleOutcome {
        if row.claim.contains("__test_pass__") {
            RuleOutcome::Verify { witness: None }
        } else if row.claim.contains("__test_fail__") {
            RuleOutcome::Fail { reason: "test sentinel: __test_fail__".to_string() }
        } else {
            RuleOutcome::Skip
        }
    }
}

// ── spawn_verifier ───────────────────────────────────────────

pub fn spawn_verifier(
    bus: Arc<MessageBus>,
    store: Arc<dyn EvidenceStore>,
    rules: Vec<Box<dyn VerifierRule>>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(poll_interval);
        let verifier_id = AgentId::new();
        info!("verifier: started with {} rule(s), polling every {}s", rules.len(), poll_interval.as_secs());
        loop {
            interval.tick().await;
            let pending = match store.pending(50).await {
                Ok(p) => p,
                Err(e) => { warn!("verifier: pending() failed: {}", e); continue; }
            };
            for row in pending {
                for rule in &rules {
                    match rule.evaluate(&row).await {
                        RuleOutcome::Skip => continue,
                        outcome => { emit_verdict(&bus, verifier_id, &row.id, outcome, rule.name()).await; break; }
                    }
                }
            }
        }
    })
}

async fn emit_verdict(bus: &MessageBus, verifier_id: AgentId, evidence_id: &str, outcome: RuleOutcome, rule_name: &str) {
    let (ok, note) = match outcome {
        RuleOutcome::Verify { .. } => (true, None),
        RuleOutcome::Fail { reason } => (false, Some(reason)),
        RuleOutcome::Skip => return,
    };
    let from = AgentAddress { agent_id: verifier_id, agent_name: format!("verifier:{}", rule_name), adapter_type: "verifier".into() };
    let msg = AgentMessage::new(from, None, MessageType::EvidenceVerify, Payload::EvidenceVerify(EvidenceVerifyMsg {
        evidence_id: evidence_id.to_string(), outcome: ok, note,
    }));
    let _ = bus.send_to_channel("general", &verifier_id, msg).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvidenceStatus;

    fn test_row(claim: &str) -> EvidenceRow {
        EvidenceRow {
            id: "ev-1".into(), task_id: "t-1".into(), correlation_id: None,
            agent_id: ch_protocol::AgentId::new(), agent_name: "test".into(),
            claim: claim.into(), status: EvidenceStatus::Pending, witness: None,
            metadata: serde_json::json!({}), created_at: chrono::Utc::now(),
            verified_at: None, verified_by: None,
        }
    }

    #[tokio::test]
    async fn keyword_rule_test_pass() {
        let row = test_row("result __test_pass__ ok");
        let rule = KeywordRule;
        match rule.evaluate(&row).await {
            RuleOutcome::Verify { .. } => {},
            _ => panic!("expected Verify"),
        }
    }

    #[tokio::test]
    async fn keyword_rule_test_fail() {
        let row = test_row("failed: __test_fail__");
        let rule = KeywordRule;
        match rule.evaluate(&row).await {
            RuleOutcome::Fail { .. } => {},
            _ => panic!("expected Fail"),
        }
    }

    #[tokio::test]
    async fn keyword_rule_skip() {
        let row = test_row("normal claim");
        let rule = KeywordRule;
        match rule.evaluate(&row).await {
            RuleOutcome::Skip => {},
            _ => panic!("expected Skip"),
        }
    }
}
