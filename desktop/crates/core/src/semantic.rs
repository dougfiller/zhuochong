use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub const RRF_K: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddingVectorError {
    #[error("embedding byte length does not match its dimension")]
    ByteLength,
    #[error("embedding dimensions do not match")]
    Dimension,
    #[error("embedding contains a non-finite value")]
    NonFinite,
    #[error("embedding has no usable magnitude")]
    ZeroNorm,
    #[error("ranked list contains a duplicate key")]
    DuplicateKey,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scored<T> {
    pub item: T,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RrfScore<K> {
    pub key: K,
    pub score: f64,
}

pub fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len().saturating_mul(4));
    for value in vector {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn decode_embedding_compatible(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

pub fn decode_embedding_exact(
    blob: &[u8],
    expected_dimension: usize,
) -> Result<Vec<f32>, EmbeddingVectorError> {
    let expected_bytes = expected_dimension
        .checked_mul(4)
        .ok_or(EmbeddingVectorError::ByteLength)?;
    if blob.len() != expected_bytes {
        return Err(EmbeddingVectorError::ByteLength);
    }
    let vector = decode_embedding_compatible(blob);
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingVectorError::NonFinite);
    }
    Ok(vector)
}

/// Screen-memory compatibility: a zero or non-finite norm leaves the vector unchanged.
pub fn normalize_embedding(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm.is_finite() && norm > f32::EPSILON {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

pub fn normalize_embedding_strict(vector: Vec<f32>) -> Result<Vec<f32>, EmbeddingVectorError> {
    if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
        return Err(if vector.is_empty() {
            EmbeddingVectorError::ZeroNorm
        } else {
            EmbeddingVectorError::NonFinite
        });
    }
    let normalized = normalize_embedding(vector);
    let norm = normalized
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return Err(EmbeddingVectorError::ZeroNorm);
    }
    Ok(normalized)
}

pub struct StreamingCosineTopK<T> {
    limit: usize,
    next_order: usize,
    values: Vec<(usize, Scored<T>)>,
}

impl<T> StreamingCosineTopK<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            next_order: 0,
            values: Vec::with_capacity(limit.saturating_add(1)),
        }
    }

    pub fn push(
        &mut self,
        item: T,
        vector: &[f32],
        query: &[f32],
    ) -> Result<(), EmbeddingVectorError> {
        if vector.len() != query.len() {
            return Err(EmbeddingVectorError::Dimension);
        }
        if vector.iter().chain(query).any(|value| !value.is_finite()) {
            return Err(EmbeddingVectorError::NonFinite);
        }
        let score = vector
            .iter()
            .zip(query)
            .map(|(left, right)| f64::from(*left) * f64::from(*right))
            .sum();
        let order = self.next_order;
        self.next_order += 1;
        if self.limit == 0 {
            return Ok(());
        }
        self.values.push((order, Scored { item, score }));
        self.values.sort_by(|left, right| {
            right
                .1
                .score
                .partial_cmp(&left.1.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        self.values.truncate(self.limit);
        Ok(())
    }

    pub fn into_sorted(self) -> Vec<Scored<T>> {
        self.values.into_iter().map(|(_, scored)| scored).collect()
    }
}

pub fn reciprocal_rank_fusion<K>(
    ranked_lists: &[Vec<K>],
    limit: usize,
) -> Result<Vec<RrfScore<K>>, EmbeddingVectorError>
where
    K: Clone + Ord,
{
    let mut scores = BTreeMap::<K, f64>::new();
    for ranked in ranked_lists {
        let mut seen = BTreeSet::new();
        for (rank, key) in ranked.iter().enumerate() {
            if !seen.insert(key) {
                return Err(EmbeddingVectorError::DuplicateKey);
            }
            *scores.entry(key.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    let mut fused = scores
        .into_iter()
        .map(|(key, score)| RrfScore { key, score })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.key.cmp(&right.key))
    });
    fused.truncate(limit);
    Ok(fused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_decode_rejects_tail_bytes_and_non_finite_values() {
        let original = vec![0.25, -1.5, 3.0];
        let encoded = encode_embedding(&original);
        assert_eq!(decode_embedding_exact(&encoded, 3).unwrap(), original);
        for tail in 1..=3 {
            let mut corrupt = encoded.clone();
            corrupt.extend(std::iter::repeat(0).take(tail));
            assert_eq!(
                decode_embedding_exact(&corrupt, 3),
                Err(EmbeddingVectorError::ByteLength)
            );
            assert_eq!(decode_embedding_compatible(&corrupt), original);
        }
        assert_eq!(
            decode_embedding_exact(&encode_embedding(&[f32::NAN]), 1),
            Err(EmbeddingVectorError::NonFinite)
        );
    }

    #[test]
    fn normalization_and_streaming_top_k_are_stable() {
        let normalized = normalize_embedding_strict(vec![3.0, 4.0]).unwrap();
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert_eq!(
            normalize_embedding_strict(vec![0.0, 0.0]),
            Err(EmbeddingVectorError::ZeroNorm)
        );
        let query = [1.0, 0.0];
        let mut top = StreamingCosineTopK::new(2);
        top.push("first", &[0.5, 0.0], &query).unwrap();
        top.push("best", &[1.0, 0.0], &query).unwrap();
        top.push("equal-first", &[0.5, 0.0], &query).unwrap();
        assert_eq!(
            top.into_sorted()
                .into_iter()
                .map(|entry| entry.item)
                .collect::<Vec<_>>(),
            vec!["best", "first"]
        );
    }

    #[test]
    fn rrf_rejects_duplicates_and_breaks_ties_by_key() {
        let fused = reciprocal_rank_fusion(&[vec!["b", "a"], vec!["a", "b"]], 2).unwrap();
        assert_eq!(fused[0].key, "a");
        assert_eq!(fused[1].key, "b");
        assert_eq!(
            reciprocal_rank_fusion(&[vec!["a", "a"]], 2),
            Err(EmbeddingVectorError::DuplicateKey)
        );
    }
}
