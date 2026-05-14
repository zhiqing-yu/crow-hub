//! Adapter implementations

pub mod claude;
pub mod codebuddy;
pub mod gemini;
pub mod hermes;
pub mod kimi;

pub use claude::ClaudeAdapter;
pub use codebuddy::CodeBuddyAdapter;
pub use gemini::GeminiAdapter;
pub use hermes::HermesAdapter;
pub use kimi::KimiAdapter;
