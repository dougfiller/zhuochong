#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmbeddingWireFormat {
    OpenAiBatch,
    OllamaLegacyPrompt,
    OllamaBatchV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmbeddingWireError {
    EmptyInput,
    InvalidResponse,
    CountMismatch,
}

pub(crate) fn embedding_payload(
    format: EmbeddingWireFormat,
    model: &str,
    texts: &[String],
) -> Result<serde_json::Value, EmbeddingWireError> {
    if model.trim().is_empty() || texts.is_empty() {
        return Err(EmbeddingWireError::EmptyInput);
    }
    Ok(match format {
        EmbeddingWireFormat::OpenAiBatch => {
            serde_json::json!({ "model": model, "input": texts })
        }
        EmbeddingWireFormat::OllamaLegacyPrompt => {
            if texts.len() != 1 {
                return Err(EmbeddingWireError::CountMismatch);
            }
            serde_json::json!({ "model": model, "prompt": texts[0] })
        }
        EmbeddingWireFormat::OllamaBatchV1 => {
            serde_json::json!({ "model": model, "input": texts })
        }
    })
}

pub(crate) fn parse_embedding_response(
    format: EmbeddingWireFormat,
    payload: serde_json::Value,
    expected_count: usize,
) -> Result<Vec<Vec<f32>>, EmbeddingWireError> {
    if expected_count == 0 {
        return Err(EmbeddingWireError::EmptyInput);
    }
    let raw_vectors = match format {
        EmbeddingWireFormat::OpenAiBatch => payload
            .get("data")
            .and_then(serde_json::Value::as_array)
            .ok_or(EmbeddingWireError::InvalidResponse)?
            .iter()
            .map(|item| item.get("embedding"))
            .collect::<Vec<_>>(),
        EmbeddingWireFormat::OllamaLegacyPrompt => {
            vec![payload.get("embedding")]
        }
        EmbeddingWireFormat::OllamaBatchV1 => payload
            .get("embeddings")
            .and_then(serde_json::Value::as_array)
            .ok_or(EmbeddingWireError::InvalidResponse)?
            .iter()
            .map(Some)
            .collect::<Vec<_>>(),
    };
    if raw_vectors.len() != expected_count {
        return Err(EmbeddingWireError::CountMismatch);
    }
    raw_vectors
        .into_iter()
        .map(|raw| {
            raw.and_then(serde_json::Value::as_array)
                .ok_or(EmbeddingWireError::InvalidResponse)?
                .iter()
                .map(|value| {
                    let number = value.as_f64().ok_or(EmbeddingWireError::InvalidResponse)?;
                    let number = number as f32;
                    if number.is_finite() {
                        Ok(number)
                    } else {
                        Err(EmbeddingWireError::InvalidResponse)
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_formats_keep_screen_shapes_and_add_ollama_batch() {
        let texts = vec!["虚构甲".to_string(), "虚构乙".to_string()];
        assert_eq!(
            embedding_payload(EmbeddingWireFormat::OpenAiBatch, "fixture", &texts).unwrap(),
            serde_json::json!({"model":"fixture","input":["虚构甲","虚构乙"]})
        );
        assert_eq!(
            parse_embedding_response(
                EmbeddingWireFormat::OpenAiBatch,
                serde_json::json!({"data":[{"embedding":[1,0]},{"embedding":[0,1]}]}),
                2,
            )
            .unwrap(),
            vec![vec![1.0, 0.0], vec![0.0, 1.0]]
        );
        assert_eq!(
            parse_embedding_response(
                EmbeddingWireFormat::OllamaBatchV1,
                serde_json::json!({"embeddings":[[1,0],[0,1]]}),
                2,
            )
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            parse_embedding_response(
                EmbeddingWireFormat::OllamaBatchV1,
                serde_json::json!({"embeddings":[[1,0]]}),
                2,
            ),
            Err(EmbeddingWireError::CountMismatch)
        );
    }
}
