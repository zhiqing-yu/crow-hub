//! Per-model pricing table for cost estimation.
//!
//! Loads from `~/.crow-hub/pricing.toml` (user-editable), falls back
//! to the checked-in `examples/pricing.toml`, then to empty.
//!
//! Match is case-insensitive substring against the model name,
//! longest-match first.  An empty `model` field is the catch-all.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Rate {
    /// Model substring to match (case-insensitive).  Empty = catch-all.
    #[serde(default)]
    pub model: String,
    /// USD per million input tokens
    pub input_per_mtok: f64,
    /// USD per million output tokens
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PricingTable {
    #[serde(default)]
    pub rates: Vec<Rate>,
}

impl PricingTable {
    /// Load pricing: user TOML → checked-in TOML → empty.
    pub fn load(home_dir: &std::path::Path) -> Self {
        // 1. User override: ~/.crow-hub/pricing.toml
        let user_path = home_dir.join("pricing.toml");
        if user_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&user_path) {
                if let Ok(table) = toml::from_str::<PricingTable>(&content) {
                    return table;
                }
            }
        }

        // 2. Checked-in default: examples/pricing.toml
        let embedded = include_str!("../../../examples/pricing.toml");
        toml::from_str::<PricingTable>(embedded).unwrap_or_default()
    }

    /// Find the best-matching rate for a model name.
    ///
    /// Returns the rate whose `model` field is a case-insensitive substring
    /// of `model_name`, preferring longer matches.  An empty `model` field
    /// matches everything and acts as the catch-all fallback.
    pub fn lookup(&self, model_name: &str) -> Option<&Rate> {
        let lower = model_name.to_lowercase();
        self.rates
            .iter()
            .filter(|r| r.model.is_empty() || lower.contains(&r.model.to_lowercase()))
            .max_by_key(|r| r.model.len())
    }

    /// Compute cost in USD for a (tokens_in, tokens_out) pair.
    pub fn cost(&self, model_name: &str, tokens_in: u64, tokens_out: u64) -> f64 {
        self.lookup(model_name)
            .map(|r| {
                (tokens_in as f64 / 1_000_000.0) * r.input_per_mtok
                    + (tokens_out as f64 / 1_000_000.0) * r.output_per_mtok
            })
            .unwrap_or(0.0)
    }
}

/// Convenience: load the user's pricing table from the standard home dir.
pub fn load_user_pricing() -> PricingTable {
    let home = crate::get_home_dir();
    PricingTable::load(&home)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_table() -> PricingTable {
        PricingTable {
            rates: vec![
                Rate { model: "claude-opus".into(), input_per_mtok: 15.0, output_per_mtok: 75.0 },
                Rate { model: "claude-sonnet".into(), input_per_mtok: 3.0, output_per_mtok: 15.0 },
                Rate { model: "gemini".into(), input_per_mtok: 1.25, output_per_mtok: 5.0 },
                Rate { model: "".into(), input_per_mtok: 0.0, output_per_mtok: 0.0 },
            ],
        }
    }

    #[test]
    fn lookup_finds_exact_match() {
        let t = sample_table();
        let r = t.lookup("claude-sonnet-4").unwrap();
        assert_eq!(r.input_per_mtok, 3.0);
    }

    #[test]
    fn lookup_longest_match_wins() {
        let t = sample_table();
        // "claude-opus" is longer than "claude" substring match
        let r = t.lookup("claude-opus-4-20250514").unwrap();
        assert_eq!(r.input_per_mtok, 15.0);
    }

    #[test]
    fn lookup_gemini() {
        let t = sample_table();
        let r = t.lookup("gemini-2.0-flash").unwrap();
        assert_eq!(r.output_per_mtok, 5.0);
    }

    #[test]
    fn lookup_fallback_empty() {
        let t = sample_table();
        let r = t.lookup("unknown-model-xyz").unwrap();
        assert_eq!(r.input_per_mtok, 0.0);
    }

    #[test]
    fn cost_calculation() {
        let t = sample_table();
        // 1M input + 1M output at claude-sonnet = $3.00 + $15.00 = $18.00
        let c = t.cost("claude-sonnet-4", 1_000_000, 1_000_000);
        assert!((c - 18.0).abs() < 0.01);
        // 100k input only at claude-sonnet = $0.30
        let c2 = t.cost("claude-sonnet-4", 100_000, 0);
        assert!((c2 - 0.30).abs() < 0.01);
    }

    #[test]
    fn empty_table_returns_zero() {
        let t = PricingTable::default();
        assert_eq!(t.cost("anything", 1_000_000, 1_000_000), 0.0);
    }

    #[test]
    fn lookup_case_insensitive() {
        let t = sample_table();
        assert!(t.lookup("CLAUDE-SONNET").is_some());
        assert!(t.lookup("Gemini-2.0").is_some());
    }
}
