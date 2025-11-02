//! Embedding vector utilities shared across memory components.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{MemoryError, MemoryResult};

/// Wrapper type around an immutable floating-point embedding.
#[derive(Clone, PartialEq)]
pub struct EmbeddingVector {
    values: Arc<[f32]>,
}

impl EmbeddingVector {
    /// Creates a new embedding from owned values.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidRecord`] when the supplied vector is empty
    /// or contains non-finite values.
    pub fn new(values: Vec<f32>) -> MemoryResult<Self> {
        if values.is_empty() {
            return Err(MemoryError::InvalidRecord(
                "embedding vector must not be empty",
            ));
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(MemoryError::InvalidRecord(
                "embedding vector contains non-finite values",
            ));
        }
        Ok(Self {
            values: Arc::<[f32]>::from(values.into_boxed_slice()),
        })
    }

    /// Creates an embedding by copying the provided slice.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidRecord`] if the slice is empty or contains
    /// non-finite values.
    pub fn from_slice(values: &[f32]) -> MemoryResult<Self> {
        Self::new(values.to_vec())
    }

    /// Returns an immutable view of the embedding data.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    /// Returns the dimensionality of the embedding.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the embedding is empty. This should never be the case
    /// because [`EmbeddingVector::new`] rejects empty inputs, but the helper is
    /// provided for completeness.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(crate) fn dot(&self, other: &Self) -> f32 {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| a * b)
            .sum()
    }

    pub(crate) fn magnitude(&self) -> f32 {
        self.values
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt()
    }
}

impl std::fmt::Debug for EmbeddingVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingVector")
            .field("dimensions", &self.len())
            .finish()
    }
}

impl Serialize for EmbeddingVector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.values.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EmbeddingVector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<f32>::deserialize(deserializer)?;
        Self::new(values).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_vectors() {
        let err = EmbeddingVector::new(vec![]).expect_err("empty vector should error");
        assert!(matches!(err, MemoryError::InvalidRecord(_)));
    }

    #[test]
    fn rejects_non_finite_values() {
        let err = EmbeddingVector::new(vec![1.0, f32::NAN]).expect_err("nan not allowed");
        assert!(matches!(err, MemoryError::InvalidRecord(_)));
    }

    #[test]
    fn serialization_roundtrip() {
        let embedding = EmbeddingVector::new(vec![0.1, 0.2, 0.3]).unwrap();
        let json = serde_json::to_string(&embedding).unwrap();
        let decoded: EmbeddingVector = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.as_slice(), embedding.as_slice());
    }
}
