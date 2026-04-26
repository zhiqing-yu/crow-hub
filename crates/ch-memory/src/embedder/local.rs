//! Local embedding model
//!
//! Uses a lightweight local model for embeddings

use super::Embedder;

/// Local embedding model
pub struct LocalEmbedder {
    dimension: usize,
}

impl LocalEmbedder {
    /// Create a new local embedder
    pub fn new() -> Self {
        Self {
            dimension: 768,
        }
    }
    
    /// Create with custom dimension
    pub fn with_dimension(dimension: usize) -> Self {
        Self { dimension }
    }
}

impl Default for LocalEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        // In a real implementation, this would use a local model
        // like sentence-transformers or a Rust-native embedding model
        // For now, we generate a deterministic pseudo-embedding
        
        let mut embedding = Vec::with_capacity(self.dimension);
        let text_hash = text.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64));
        
        for i in 0..self.dimension {
            let value = ((text_hash.wrapping_add(i as u64) as f64) % 1000.0) / 1000.0;
            embedding.push(value as f32);
        }
        
        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }
        
        Ok(embedding)
    }
    
    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
    
    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embedder() {
        let embedder = LocalEmbedder::new();
        
        let embedding = embedder.embed("Hello world").await.unwrap();
        assert_eq!(embedding.len(), 768);
        
        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01 || norm < 0.01);
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let embedder = LocalEmbedder::new();
        
        let texts = vec![
            "Hello world".to_string(),
            "Test text".to_string(),
        ];
        
        let embeddings = embedder.embed_batch(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 768);
    }
}
