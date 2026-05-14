//! Model Backend implementations

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai_compat;

pub use anthropic::AnthropicBackend;
pub use mock::MockBackend;
pub use ollama::OllamaBackend;
pub use openai_compat::OpenAICompatBackend;
