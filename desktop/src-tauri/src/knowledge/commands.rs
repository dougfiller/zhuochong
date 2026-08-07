use super::config::validate_local_embedding;
use super::KnowledgeStore;
use crate::config::{KnowledgeConfig, LocalEmbeddingConfig};
use crate::error::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelectedRoot {
    selected_root: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelectedRoots {
    selection_receipt_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OperationQuery {
    operation_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceMutation {
    source_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeSourcesStatus {
    sources: Vec<super::store::KnowledgeSourceStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeMutationReceipt {
    ok: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeOperationReceipt {
    operation_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeRebuildSelectionReceipt {
    selection_receipt_id: String,
}

fn command_error() -> AppError {
    AppError::Unknown("KB_NOT_READY".into())
}

fn rebuild_error(error: crate::wechat::types::ContractError) -> AppError {
    let code = match error {
        crate::wechat::types::ContractError::KbRebuildSelectionRequired => {
            "KB_REBUILD_SELECTION_REQUIRED"
        }
        _ => "KB_NOT_READY",
    };
    AppError::Unknown(code.into())
}

#[tauri::command]
pub(crate) async fn get_knowledge_settings_status(
    store: State<'_, KnowledgeStore>,
    binding: State<'_, crate::wechat::binding::KnowledgeScopeBinding>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<KnowledgeSettingsStatus, AppError> {
    let config = {
        let state = state
            .lock()
            .map_err(|error| AppError::Unknown(error.to_string()))?;
        state.config.knowledge.clone()
    };
    let mut status = status_for(&config);
    status.scope_selected = binding
        .status()
        .map(|status| status.state == "bound")
        .unwrap_or(false);
    let sources = store.list_sources().map_err(|_| command_error())?;
    let maintenance = store.maintenance_status().map_err(|_| command_error())?;
    status.source_count = sources.len();
    status.active_source_count = sources
        .iter()
        .filter(|source| source.source_state == "active")
        .count();
    if maintenance.maintenance == "closed" {
        status.not_ready_reason = "KB_MAINTENANCE";
    } else if status.active_source_count > 0
        && status.scope_selected
        && status.local_embedding_valid
    {
        status.not_ready_reason = "KB_NOT_READY";
    }
    Ok(status)
}

#[tauri::command]
pub(crate) async fn list_knowledge_sources(
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeSourcesStatus, AppError> {
    Ok(KnowledgeSourcesStatus {
        sources: store.list_sources().map_err(|_| command_error())?,
    })
}

#[tauri::command]
pub(crate) async fn list_knowledge_conversations(
    store: State<'_, KnowledgeStore>,
) -> Result<super::store::ActiveConversationCatalog, AppError> {
    store.list_active_conversations().map_err(|error| {
        AppError::Unknown(
            serde_json::to_string(&error).unwrap_or_else(|_| "\"KB_RETRIEVAL_FAILED\"".into()),
        )
    })
}

#[tauri::command]
pub(crate) async fn get_knowledge_maintenance_status(
    input: OperationQuery,
    store: State<'_, KnowledgeStore>,
) -> Result<super::store::MaintenanceStatus, AppError> {
    store
        .maintenance_status_for(input.operation_id.as_deref())
        .map_err(|_| command_error())
}

#[tauri::command]
pub(crate) async fn start_knowledge_source_import(
    input: SelectedRoot,
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeOperationReceipt, AppError> {
    let root = PathBuf::from(input.selected_root);
    if root.as_os_str().is_empty() {
        return Err(command_error());
    }
    let operation_id = store
        .start_source_import(root)
        .map_err(|_| command_error())?;
    Ok(KnowledgeOperationReceipt { operation_id })
}

#[tauri::command]
pub(crate) async fn retire_knowledge_source(
    input: SourceMutation,
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeMutationReceipt, AppError> {
    if input.source_id.is_empty() {
        return Err(command_error());
    }
    store
        .retire_source(&input.source_id)
        .map_err(|_| command_error())?;
    Ok(KnowledgeMutationReceipt { ok: true })
}

#[tauri::command]
pub(crate) async fn deny_knowledge_source(
    input: SourceMutation,
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeMutationReceipt, AppError> {
    if input.source_id.is_empty() {
        return Err(command_error());
    }
    store
        .deny_source(&input.source_id)
        .map_err(|_| command_error())?;
    Ok(KnowledgeMutationReceipt { ok: true })
}

#[tauri::command]
pub(crate) async fn pick_knowledge_rebuild_roots(
    app: AppHandle,
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeRebuildSelectionReceipt, AppError> {
    let roots = app
        .dialog()
        .file()
        .blocking_pick_folders()
        .ok_or_else(command_error)?
        .into_iter()
        .map(|path| path.into_path().map_err(|_| command_error()))
        .collect::<Result<Vec<PathBuf>, AppError>>()?;
    let selection_receipt_id = store
        .issue_rebuild_selection(roots)
        .map_err(rebuild_error)?;
    Ok(KnowledgeRebuildSelectionReceipt {
        selection_receipt_id,
    })
}

#[tauri::command]
pub(crate) async fn start_knowledge_rebuild(
    input: SelectedRoots,
    store: State<'_, KnowledgeStore>,
) -> Result<KnowledgeOperationReceipt, AppError> {
    if input.selection_receipt_id.is_empty() {
        return Err(command_error());
    }
    let operation_id = store
        .prepare_and_start_rebuild(&input.selection_receipt_id)
        .map_err(rebuild_error)?;
    Ok(KnowledgeOperationReceipt { operation_id })
}

#[tauri::command]
pub(crate) async fn validate_knowledge_local_embedding(
    local_embedding: LocalEmbeddingConfig,
) -> LocalEmbeddingValidation {
    match validate_local_embedding(&local_embedding) {
        Ok(()) => LocalEmbeddingValidation {
            valid: true,
            error_code: None,
        },
        Err(error_code) => LocalEmbeddingValidation {
            valid: false,
            error_code: Some(error_code),
        },
    }
}

fn status_for(config: &KnowledgeConfig) -> KnowledgeSettingsStatus {
    let embedding = validate_local_embedding(&config.local_embedding);
    let scope_selected = false;
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
            .filter(|source| {
                matches!(
                    source.source_state,
                    crate::config::KnowledgeSourceState::Active
                )
            })
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
            assert_eq!(
                result.error_code,
                Some("KB_EMBEDDING_ENDPOINT_NOT_LOOPBACK")
            );
        }
    }
}
