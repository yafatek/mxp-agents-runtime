//! Vector store traits and a local in-memory implementation.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::embeddings::EmbeddingVector;
use crate::MemoryResult;

/// Record stored in a vector database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPoint {
    id: Uuid,
    embedding: EmbeddingVector,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    tags: Vec<String>,
}

impl VectorPoint {
    /// Creates a new vector point with optional metadata.
    #[must_use]
    pub fn new(id: Uuid, embedding: EmbeddingVector) -> Self {
        Self {
            id,
            embedding,
            metadata: Value::Null,
            tags: Vec::new(),
        }
    }

    /// Assigns metadata to the point.
    #[must_use]
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Assigns tags to the point.
    #[must_use]
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the identifier.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the embedding reference.
    #[must_use]
    pub fn embedding(&self) -> &EmbeddingVector {
        &self.embedding
    }

    /// Returns tags associated with the point.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Returns the metadata payload.
    #[must_use]
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
}

/// Query parameters for retrieving similar vectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorQuery {
    embedding: EmbeddingVector,
    top_k: NonZeroUsize,
    #[serde(default)]
    tags: Vec<String>,
}

impl VectorQuery {
    /// Creates a new query request.
    #[must_use]
    pub fn new(embedding: EmbeddingVector, top_k: NonZeroUsize) -> Self {
        Self {
            embedding,
            top_k,
            tags: Vec::new(),
        }
    }

    /// Restricts results to vectors tagged with all provided labels.
    #[must_use]
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the embedding driving the query.
    #[must_use]
    pub fn embedding(&self) -> &EmbeddingVector {
        &self.embedding
    }

    /// Returns the desired number of results.
    #[must_use]
    pub fn top_k(&self) -> usize {
        self.top_k.get()
    }

    /// Returns tags to enforce during search.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Match returned from a vector store query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMatch {
    id: Uuid,
    score: f32,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    tags: Vec<String>,
}

impl VectorMatch {
    /// Creates a match structure.
    #[must_use]
    pub fn new(id: Uuid, score: f32, metadata: Value, tags: Vec<String>) -> Self {
        Self {
            id,
            score,
            metadata,
            tags,
        }
    }

    /// Returns the identifier.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns cosine similarity score.
    #[must_use]
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Returns metadata payload.
    #[must_use]
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }

    /// Returns tags associated with the match.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Interface for vector store clients.
#[async_trait]
pub trait VectorStoreClient: Send + Sync {
    /// Inserts or updates a vector point.
    async fn upsert(&self, point: VectorPoint) -> MemoryResult<()>;

    /// Removes a vector point if present.
    async fn remove(&self, id: Uuid) -> MemoryResult<()>;

    /// Executes a similarity query and returns matches ordered by descending score.
    async fn query(&self, query: VectorQuery) -> MemoryResult<Vec<VectorMatch>>;
}

/// Simple in-memory vector store using cosine similarity.
pub struct LocalVectorStore {
    points: RwLock<HashMap<Uuid, VectorPoint>>,
}

impl LocalVectorStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            points: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for LocalVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStoreClient for LocalVectorStore {
    async fn upsert(&self, point: VectorPoint) -> MemoryResult<()> {
        let mut guard = self.points.write().await;
        guard.insert(point.id(), point);
        Ok(())
    }

    async fn remove(&self, id: Uuid) -> MemoryResult<()> {
        let mut guard = self.points.write().await;
        guard.remove(&id);
        Ok(())
    }

    async fn query(&self, query: VectorQuery) -> MemoryResult<Vec<VectorMatch>> {
        let guard = self.points.read().await;
        let mut matches = Vec::new();

        let query_embedding = query.embedding();
        let query_tags = query.tags();

        for point in guard.values() {
            if !query_tags.is_empty()
                && !query_tags
                    .iter()
                    .all(|tag| point.tags().iter().any(|candidate| candidate == tag))
            {
                continue;
            }

            if point.embedding().len() != query_embedding.len() {
                continue;
            }

            let score = cosine_similarity(point.embedding(), query_embedding);
            matches.push(VectorMatch::new(
                point.id(),
                score,
                point.metadata().clone(),
                point.tags().to_vec(),
            ));
        }

        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(query.top_k());
        Ok(matches)
    }
}

fn cosine_similarity(lhs: &EmbeddingVector, rhs: &EmbeddingVector) -> f32 {
    let numerator = lhs.dot(rhs);
    let denominator = lhs.magnitude() * rhs.magnitude();
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_query() {
        let store = LocalVectorStore::new();

        store
            .upsert(
                VectorPoint::new(
                    Uuid::new_v4(),
                    EmbeddingVector::new(vec![1.0, 0.0, 0.0]).unwrap(),
                )
                .with_tags(["alpha"]),
            )
            .await
            .unwrap();

        store
            .upsert(
                VectorPoint::new(
                    Uuid::new_v4(),
                    EmbeddingVector::new(vec![0.0, 1.0, 0.0]).unwrap(),
                )
                .with_tags(["beta"]),
            )
            .await
            .unwrap();

        let query = VectorQuery::new(
            EmbeddingVector::new(vec![1.0, 0.0, 0.0]).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        );
        let matches = store.query(query).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tags(), ["alpha"]);
        assert!((matches[0].score() - 1.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn respects_tag_filter() {
        let store = LocalVectorStore::new();
        let id = Uuid::new_v4();
        store
            .upsert(
                VectorPoint::new(id, EmbeddingVector::new(vec![1.0, 1.0]).unwrap())
                    .with_tags(["alpha", "beta"]),
            )
            .await
            .unwrap();

        let query = VectorQuery::new(
            EmbeddingVector::new(vec![1.0, 1.0]).unwrap(),
            NonZeroUsize::new(5).unwrap(),
        )
        .with_tags(["beta", "alpha"]);
        let matches = store.query(query).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id(), id);
    }
}
