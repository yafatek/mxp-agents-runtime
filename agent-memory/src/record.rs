//! Shared record types for the memory subsystem.

use std::time::SystemTime;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::embeddings::EmbeddingVector;
use crate::{MemoryError, MemoryResult};

/// Channel categorising a memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChannel {
    /// Messages originating from outside the agent (e.g. MXP Call payloads).
    Input,
    /// Messages produced by the agent (responses to MXP calls).
    Output,
    /// Tool invocation results or intermediate tool state.
    Tool,
    /// Internal agent/runtime events (checkpoints, policy results, etc.).
    System,
    /// Custom channel tagged by implementers for domain-specific routing.
    Custom(String),
}

impl MemoryChannel {
    /// Creates a [`MemoryChannel::Custom`] value after validating the provided name.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidRecord`] when the supplied label is empty.
    pub fn custom(label: impl Into<String>) -> MemoryResult<Self> {
        let value = label.into();
        if value.trim().is_empty() {
            return Err(MemoryError::InvalidRecord(
                "custom memory channel label must not be empty",
            ));
        }
        Ok(Self::Custom(value))
    }
}

/// Describes a single captured piece of memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    id: Uuid,
    timestamp: SystemTime,
    channel: MemoryChannel,
    payload: Bytes,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding: Option<EmbeddingVector>,
}

impl MemoryRecord {
    /// Creates a builder for a new memory record.
    #[must_use]
    pub fn builder(channel: MemoryChannel, payload: Bytes) -> MemoryRecordBuilder {
        MemoryRecordBuilder {
            id: Uuid::new_v4(),
            timestamp: SystemTime::now(),
            channel,
            payload,
            tags: Vec::new(),
            metadata: Map::new(),
            embedding: None,
        }
    }

    /// Returns the unique identifier for this record.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the timestamp associated with the record.
    #[must_use]
    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }

    /// Returns the channel.
    #[must_use]
    pub fn channel(&self) -> &MemoryChannel {
        &self.channel
    }

    /// Returns the payload bytes.
    #[must_use]
    pub fn payload(&self) -> &Bytes {
        &self.payload
    }

    /// Returns associated tags.
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Returns metadata map.
    #[must_use]
    pub fn metadata(&self) -> &Map<String, Value> {
        &self.metadata
    }

    /// Returns the optional embedding associated with the record.
    #[must_use]
    pub fn embedding(&self) -> Option<&EmbeddingVector> {
        self.embedding.as_ref()
    }
}

/// Builder type used to assemble [`MemoryRecord`] instances safely.
#[derive(Debug)]
pub struct MemoryRecordBuilder {
    id: Uuid,
    timestamp: SystemTime,
    channel: MemoryChannel,
    payload: Bytes,
    tags: Vec<String>,
    metadata: Map<String, Value>,
    embedding: Option<EmbeddingVector>,
}

impl MemoryRecordBuilder {
    /// Overrides the record identifier.
    #[must_use]
    pub fn id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    /// Sets the timestamp for the record.
    #[must_use]
    pub fn timestamp(mut self, timestamp: SystemTime) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Adds a single tag after validating that it is not empty.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidRecord`] when the tag is empty or whitespace.
    pub fn tag(mut self, tag: impl Into<String>) -> MemoryResult<Self> {
        let value = tag.into();
        if value.trim().is_empty() {
            return Err(MemoryError::InvalidRecord("memory tags must not be empty"));
        }
        self.tags.push(value);
        Ok(self)
    }

    /// Extends the record with multiple tags.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidRecord`] if any supplied tag is empty.
    pub fn tags<I, S>(mut self, tags: I) -> MemoryResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            self = self.tag(tag)?;
        }
        Ok(self)
    }

    /// Adds metadata entry.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Adds a full metadata map, overwriting existing keys when duplicates occur.
    #[must_use]
    pub fn merge_metadata(mut self, map: Map<String, Value>) -> Self {
        self.metadata.extend(map);
        self
    }

    /// Attaches an embedding to the record.
    #[must_use]
    pub fn embedding(mut self, embedding: EmbeddingVector) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Finalises the builder and produces the record.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when the builder state fails validation.
    pub fn build(self) -> MemoryResult<MemoryRecord> {
        Ok(MemoryRecord {
            id: self.id,
            timestamp: self.timestamp,
            channel: self.channel,
            payload: self.payload,
            tags: self.tags,
            metadata: self.metadata,
            embedding: self.embedding,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_rejects_empty_tags() {
        let payload = Bytes::from_static(b"payload");
        let err = MemoryRecord::builder(MemoryChannel::Input, payload.clone())
            .tag("")
            .expect_err("empty tag should fail");
        assert!(matches!(err, MemoryError::InvalidRecord(_)));

        let err = MemoryRecord::builder(MemoryChannel::Input, payload)
            .tags(vec!["ok", " "])
            .expect_err("whitespace tag should fail");
        assert!(matches!(err, MemoryError::InvalidRecord(_)));
    }

    #[test]
    fn builder_constructs_record() {
        let payload = Bytes::from_static(b"payload");
        let record = MemoryRecord::builder(MemoryChannel::Output, payload.clone())
            .tag("mxp")
            .unwrap()
            .metadata("key", Value::from("value"))
            .build()
            .unwrap();

        assert_eq!(record.payload(), &payload);
        assert_eq!(record.tags(), ["mxp"]);
        assert_eq!(record.metadata().get("key").unwrap(), "value");
    }
}
