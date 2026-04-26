//! Model Backend implementations

pub mod openai_compat;
pub mod anthropic;
pub mod ollama;
pub mod mock;

pub use openai_compat::OpenAICompatBackend;
pub use anthropic::AnthropicBackend;
pub use ollama::OllamaBackend;
pub use mock::MockBackend;
