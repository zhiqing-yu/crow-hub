//! Embedding models for vectorization

pub mod local;

/// Embedder trait for generating vector embeddings
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    /// Generate embedding for text
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    
    /// Generate embeddings for multiple texts
    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    
    /// Get embedding dimension
    fn dimension(&self) -> usize;
}
