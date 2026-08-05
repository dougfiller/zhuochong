//! 助手会话持久化命令（P3）。
//!
//! 会话与消息存 SQLite（`assistant_conversations` / `assistant_messages`），
//! 替代此前 localStorage 40 条上限的易失存储。前端在每轮问答完成后调用
//! `append_assistant_message` 存档，打开助手页时按会话加载。

use crate::database::{AssistantConversation, AssistantStoredMessage};
use crate::error::AppError;
use crate::AppState;
use std::sync::{Arc, Mutex};
use tauri::State;

/// 会话列表（按最近更新排序）。
#[tauri::command]
pub async fn list_assistant_conversations(
    limit: Option<u32>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<AssistantConversation>, AppError> {
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database
        .list_assistant_conversations(limit.unwrap_or(50) as usize)
}

/// 新建会话，返回会话 id。
#[tauri::command]
pub async fn create_assistant_conversation(
    title: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<i64, AppError> {
    let trimmed = title.trim();
    let title = if trimmed.is_empty() { "新对话" } else { trimmed };
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database.create_assistant_conversation(title)
}

/// 读取会话消息。
#[tauri::command]
pub async fn get_assistant_messages(
    conversation_id: i64,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<AssistantStoredMessage>, AppError> {
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database.get_assistant_messages(conversation_id)
}

/// 追加一条消息（role: user/assistant），返回消息 id。
#[tauri::command]
pub async fn append_assistant_message(
    conversation_id: i64,
    role: String,
    content: String,
    tool_digest: Option<String>,
    model_name: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<i64, AppError> {
    if role != "user" && role != "assistant" {
        return Err(AppError::Unknown(format!("非法的消息角色: {role}")));
    }
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database.append_assistant_message(
        conversation_id,
        &role,
        &content,
        tool_digest.as_deref(),
        model_name.as_deref(),
    )
}


/// 删除会话及全部消息。
#[tauri::command]
pub async fn delete_assistant_conversation(
    conversation_id: i64,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database.delete_assistant_conversation(conversation_id)
}
