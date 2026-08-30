//! Vector embedding for the knowledge base.
//!
//! The plan targets `fastembed` (local, offline ONNX embeddings). To keep the
//! CI and the first release dependency-light, the embedding layer is a trait
//! with two backends:
//!
//! - `HashedEmbedder` (default): a deterministic, dependency-free bag-of-words
//!   hashing embedder. It is *not* semantically meaningful the way a real
//!   embedding model is, but it exercises the full retrieval pipeline
//!   (vector index + cosine similarity + RRF fusion) offline.
//! - `FastembedBackend` (feature `fastembed`): real local embeddings via the
//!   `fastembed` crate (onnxruntime), gated behind a cargo feature so CI and
//!   minimal builds stay fast.
//!
//! This is the plan's documented degradation path: full-text + vector hybrid
//! search works end to end with either backend.

/// A vector embedding: fixed-dimension dense vector.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Embedding {
    pub dim: usize,
    /// Dense floats. For the hashed backend most entries are 0 (sparse-ish).
    pub values: Vec<f32>,
}

/// Cosine similarity in [0, 1].
pub fn cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    let n = a.values.len().min(b.values.len());
    for i in 0..n {
        dot += a.values[i] * b.values[i];
        na += a.values[i] * a.values[i];
        nb += b.values[i] * b.values[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Embeds text into a fixed-dimension vector.
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Embedding;
}

/// Deterministic hashing embedder: hashes token n-grams into a fixed bag.
pub struct HashedEmbedder {
    dim: usize,
}

impl HashedEmbedder {
    pub const DEFAULT_DIM: usize = 256;

    pub fn new() -> Self {
        HashedEmbedder { dim: Self::DEFAULT_DIM }
    }
}

impl Default for HashedEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for HashedEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Embedding {
        let mut values = vec![0.0f32; self.dim];
        // tokenize on non-alphanumeric; also split CJK into 2-grams
        let tokens: Vec<String> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty())
            .map(|t| {
                if t.chars().all(|c| c.is_ascii_alphanumeric()) {
                    t.to_string()
                } else {
                    // CJK: character bigrams
                    let chars: Vec<char> = t.chars().collect();
                    if chars.len() == 1 {
                        t.to_string()
                    } else {
                        chars
                            .windows(2)
                            .map(|w| w.iter().collect::<String>())
                            .collect::<Vec<_>>()
                            .join("|")
                    }
                }
            })
            .collect();

        for token in &tokens {
            let h = hash_token(token) % self.dim as u64;
            values[h as usize] += 1.0;
        }
        // L2 normalize
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in values.iter_mut() {
                *v /= norm;
            }
        }
        Embedding { dim: self.dim, values }
    }
}

/// FNV-1a hash for stable token → dimension mapping.
fn hash_token(token: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in token.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The selected embedder (feature-gated): fastembed when enabled, hashed
/// otherwise. Kept behind a constructor so the rest of the code talks to
/// `Embedder` only.
pub fn default_embedder() -> Box<dyn Embedder> {
    #[cfg(feature = "fastembed")]
    {
        Box::new(FastembedBackend::new())
    }
    #[cfg(not(feature = "fastembed"))]
    {
        Box::new(HashedEmbedder::new())
    }
}

#[cfg(feature = "fastembed")]
pub struct FastembedBackend {
    model: fastembed::TextEmbedding,
}

#[cfg(feature = "fastembed")]
impl FastembedBackend {
    pub fn new() -> Self {
        let model = fastembed::TextEmbedding::try_new(fastembed::InitOptions {
            model_name: fastembed::ModelName::BGEBaseEN,
            ..Default::default()
        })
        .expect("fastembed model init");
        FastembedBackend { model }
    }
}

#[cfg(feature = "fastembed")]
impl Embedder for FastembedBackend {
    fn dim(&self) -> usize {
        768
    }

    fn embed(&self, text: &str) -> Embedding {
        let v = self
            .model
            .embed(vec![text], None)
            .into_iter()
            .next()
            .unwrap_or_default();
        Embedding { dim: v.len(), values: v }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_embedder_is_deterministic_and_normalized() {
        let e = HashedEmbedder::new();
        let a = e.embed("知屿 知识库 检索");
        let b = e.embed("知屿 知识库 检索");
        assert_eq!(a, b);
        assert_eq!(a.dim, 256);
        let norm = a.values.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }

    #[test]
    fn similar_texts_embed_similarly() {
        let e = HashedEmbedder::new();
        let a = e.embed("Rust 语言 所有权 借用");
        let b = e.embed("Rust 所有权 借用 生命周期");
        let c = e.embed("天气预报 明天 下雨");
        let sim_ab = cosine_similarity(&a, &b);
        let sim_ac = cosine_similarity(&a, &c);
        assert!(sim_ab > sim_ac, "similar texts should score higher: {sim_ab} vs {sim_ac}");
    }

    #[test]
    fn cosine_of_identical_is_one() {
        let e = HashedEmbedder::new();
        let a = e.embed("same");
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-4);
    }
}
