use crate::model::{LearningStrategy, SignalSource, SkillPackage};
use crate::state::SkillState;
use std::collections::HashMap;

/// 自适应学习引擎
pub struct AdaptiveEngine;

impl AdaptiveEngine {
    /// 处理学习信号
    pub fn process_signal(
        state: &mut SkillState,
        package: &SkillPackage,
        signal: SignalSource,
        context: &HashMap<String, serde_json::Value>,
    ) -> AdaptiveResult {
        if !package.adaptive.enabled || state.learning_paused {
            return AdaptiveResult::Skipped;
        }

        let config = &package.adaptive;
        let mut updates = Vec::new();

        for field in &config.learnable_fields {
            if !field.signal_sources.contains(&signal) {
                continue;
            }

            let field_path = &field.field_path;
            let current_value = state
                .learned_params
                .get(field_path)
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let new_value = match field.strategy {
                LearningStrategy::Frequency => Self::frequency_update(
                    &current_value,
                    context,
                    field_path,
                    config.max_change_rate,
                ),
                LearningStrategy::WeightedAverage => Self::weighted_avg_update(
                    &current_value,
                    context,
                    field_path,
                    config.max_change_rate,
                ),
                LearningStrategy::ThresholdTuning => Self::threshold_update(
                    &current_value,
                    context,
                    field_path,
                    config.max_change_rate,
                    signal.clone(),
                ),
                LearningStrategy::RankPreference => {
                    Self::rank_update(&current_value, context, field_path, signal.clone())
                }
            };

            if let Some(nv) = new_value {
                let confidence =
                    Self::compute_confidence(&current_value, &nv, config.max_change_rate);
                state.record_learning(
                    signal.clone(),
                    field_path.clone(),
                    current_value.clone(),
                    nv.clone(),
                    confidence,
                    100,
                );
                state.learned_params.insert(field_path.clone(), nv.clone());
                updates.push(FieldUpdate {
                    field_path: field_path.clone(),
                    old_value: current_value,
                    new_value: nv,
                    confidence,
                });
            }
        }

        if updates.is_empty() {
            AdaptiveResult::NoChange
        } else {
            AdaptiveResult::Updated(updates)
        }
    }

    /// 回滚到指定时间之前的状态
    pub fn rollback(state: &mut SkillState, before_timestamp: u64) -> usize {
        let original_len = state.learning_history.len();
        let to_keep: Vec<_> = state
            .learning_history
            .drain(..)
            .filter(|r| r.timestamp >= before_timestamp)
            .collect();
        let rolled_back = original_len - to_keep.len();
        state.learning_history = to_keep;

        // 重建 learned_params from remaining history
        state.learned_params.clear();
        for record in &state.learning_history {
            state
                .learned_params
                .insert(record.field_path.clone(), record.new_value.clone());
        }

        rolled_back
    }

    /// 暂停学习
    pub fn pause_learning(state: &mut SkillState) {
        state.learning_paused = true;
    }

    /// 恢复学习
    pub fn resume_learning(state: &mut SkillState) {
        state.learning_paused = false;
    }

    fn frequency_update(
        current: &serde_json::Value,
        context: &HashMap<String, serde_json::Value>,
        field_path: &str,
        max_change: f64,
    ) -> Option<serde_json::Value> {
        let new_val = context.get(field_path)?;
        match (current, new_val) {
            (serde_json::Value::Number(n), serde_json::Value::Number(m)) => {
                let n = n.as_f64()?;
                let m = m.as_f64()?;
                let delta = (m - n).abs();
                if delta / n.max(1.0) <= max_change {
                    Some(serde_json::json!((n + m) / 2.0))
                } else {
                    let clamped = n + (m - n).signum() * n.max(1.0) * max_change;
                    Some(serde_json::json!(clamped))
                }
            }
            _ => Some(new_val.clone()),
        }
    }

    fn weighted_avg_update(
        current: &serde_json::Value,
        context: &HashMap<String, serde_json::Value>,
        field_path: &str,
        max_change: f64,
    ) -> Option<serde_json::Value> {
        let weight = 0.3;
        let new_val = context.get(field_path)?;
        match (current, new_val) {
            (serde_json::Value::Number(n), serde_json::Value::Number(m)) => {
                let n = n.as_f64()?;
                let m = m.as_f64()?;
                let avg = n * (1.0 - weight) + m * weight;
                let delta = (avg - n).abs();
                if delta / n.max(1.0) <= max_change {
                    Some(serde_json::json!(avg))
                } else {
                    let clamped = n + (avg - n).signum() * n.max(1.0) * max_change;
                    Some(serde_json::json!(clamped))
                }
            }
            _ => Some(new_val.clone()),
        }
    }

    fn threshold_update(
        current: &serde_json::Value,
        context: &HashMap<String, serde_json::Value>,
        field_path: &str,
        max_change: f64,
        signal: SignalSource,
    ) -> Option<serde_json::Value> {
        let adjustment = match signal {
            SignalSource::AcceptSuggestion => 0.05,
            SignalSource::IgnoreSuggestion => -0.05,
            _ => return None,
        };
        match current {
            serde_json::Value::Number(n) => {
                let n = n.as_f64()?;
                let new_val = n + adjustment;
                let delta = (new_val - n).abs();
                if delta <= max_change {
                    Some(serde_json::json!(new_val))
                } else {
                    None
                }
            }
            _ => context.get(field_path).cloned(),
        }
    }

    fn rank_update(
        current: &serde_json::Value,
        context: &HashMap<String, serde_json::Value>,
        field_path: &str,
        signal: SignalSource,
    ) -> Option<serde_json::Value> {
        let adjustment = match signal {
            SignalSource::AcceptSuggestion | SignalSource::FollowupAction => 1,
            SignalSource::IgnoreSuggestion => -1,
            _ => 0,
        };
        if adjustment == 0 {
            return None;
        }
        match current {
            serde_json::Value::Number(n) => {
                let n = n.as_i64()?;
                Some(serde_json::json!(n + adjustment))
            }
            _ => context.get(field_path).cloned(),
        }
    }

    fn compute_confidence(
        old: &serde_json::Value,
        new: &serde_json::Value,
        max_change: f64,
    ) -> f64 {
        match (old.as_f64(), new.as_f64()) {
            (Some(o), Some(n)) if o > 0.0 => {
                let change = (n - o).abs() / o;
                1.0 - (change / max_change).min(1.0)
            }
            _ => 0.5,
        }
    }
}

/// 自适应更新结果
#[derive(Debug)]
pub enum AdaptiveResult {
    /// 更新成功
    Updated(Vec<FieldUpdate>),
    /// 无变化
    NoChange,
    /// 已跳过（暂停或不启用）
    Skipped,
}

/// 字段更新详情
#[derive(Debug)]
pub struct FieldUpdate {
    pub field_path: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
    pub confidence: f64,
}
