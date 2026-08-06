use super::config::validate_local_embedding;
use super::KnowledgeStore;
use crate::config::{KnowledgeConfig, LocalEmbeddingConfig};
use crate::error::AppError;
use crate::AppState;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeSettingsStatus {
    not_ready_reason: &'static str,
    source_count: usize,
    active_source_count: usize,
    scope_selected: bool,
    local_embedding_valid: bool,
    local_embedding_error: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalEmbeddingValidation {
    valid: bool,
    error_code: Option<&'static str>,
}

#[tauri::command]
pub(crate) async fn get_knowledge_settings_status(
    _store: State<'_, KnowledgeStore>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<KnowledgeSettingsStatus, AppError> {
    let config = {
        let state = state.lock().map_err(|error| AppError::Unknown(error.to_string()))?;
        state.config.knowledge.clone()
    };
    Ok(status_for(&config))
}

#[tauri::command]
pub(crate) async fn validate_knowledge_local_embedding(
    local_embedding: LocalEmbeddingConfig,
) -> LocalEmbeddingValidation {
    match validate_local_embedding(&local_embedding) {
        Ok(()) => LocalEmbeddingValidation { valid: true, error_code: None },
        Err(error_code) => LocalEmbeddingValidation { valid: false, error_code: Some(error_code) },
    }
}

fn status_for(config: &KnowledgeConfig) -> KnowledgeSettingsStatus {
    let embedding = validate_local_embedding(&config.local_embedding);
    let scope_selected = config.scope_mode.is_some();
    let local_embedding_error = embedding.err();
    let not_ready_reason = if !scope_selected {
        "KB_SCOPE_UNRESOLVED"
    } else if config.knowledge_sources.is_empty() {
        "KB_NOT_READY"
    } else if local_embedding_error.is_some() {
        local_embedding_error.expect("checked")
    } else {
        "KB_NOT_READY"
    };
    KnowledgeSettingsStatus {
        not_ready_reason,
        source_count: config.knowledge_sources.len(),
        active_source_count: config
            .knowledge_sources
            .iter()
            .filter(|source| matches!(source.source_state, crate::config::KnowledgeSourceState::Active))
            .count(),
        scope_selected,
        local_embedding_valid: local_embedding_error.is_none(),
        local_embedding_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_scope_never_means_global_scope() {
        let status = status_for(&KnowledgeConfig::default());
        assert!(!status.scope_selected);
        assert_eq!(status.not_ready_reason, "KB_SCOPE_UNRESOLVED");
    }

    #[tokio::test]
    async fn validation_command_rejects_non_loopback_url_bypasses() {
        for endpoint in [
            "http://localhost:80@evil.example",
            "http://127.0.0.1:80@evil.example",
            "http://user:password@localhost",
            "http://localhost:65536",
        ] {
            let result = validate_knowledge_local_embedding(LocalEmbeddingConfig {
                provider: "ollama_loopback".into(),
                endpoint: endpoint.into(),
                model: "nomic".into(),
            })
            .await;

            assert!(!result.valid);
            assert_eq!(result.error_code, Some("KB_EMBEDDING_ENDPOINT_NOT_LOOPBACK"));
        }
    }
}
