//! Auto-extracted from the historical `commands.rs`. Behavior unchanged.

use crate::error::AppError;
use crate::AppState;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};



/// 暂停录制
#[tauri::command]
pub async fn pause_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let mut state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    state.is_paused = true;
    log::info!("暂停录制");
    drop(state);
    crate::emit_recording_state_changed(&app);
    Ok(())
}

/// 恢复录制
#[tauri::command]
pub async fn resume_recording(
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let mut state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    state.is_recording = true;
    state.is_paused = false;
    log::info!("恢复录制");
    drop(state);
    crate::emit_recording_state_changed(&app);
    Ok(())
}

/// 获取录制状态
#[tauri::command]
pub async fn get_recording_state(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(bool, bool), AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok((state.is_recording, state.is_paused))
}






