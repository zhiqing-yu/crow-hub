//! Adapter implementations

pub mod claude;
pub mod kimi;
pub mod gemini;
pub mod hermes;
pub mod codebuddy;

pub use claude::ClaudeAdapter;
pub use kimi::KimiAdapter;
pub use gemini::GeminiAdapter;
pub use hermes::HermesAdapter;
pub use codebuddy::CodeBuddyAdapter;
