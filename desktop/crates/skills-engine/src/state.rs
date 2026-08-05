use crate::model::{SignalSource, SkillId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 技能状态（用户态，可学习）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillState {
    pub skill_id: SkillId,
    /// 学习得到的参数
    #[serde(default)]
    pub learned_params: HashMap<String, serde_json::Value>,
    /// 学习历史（最近 N 条）
    #[serde(default)]
    pub learning_history: Vec<LearningRecord>,
    /// 统计计数
    #[serde(default)]
    pub stats: SkillStats,
    /// 是否暂停学习
    #[serde(default)]
    pub learning_paused: bool,
    /// 上次学习时间（Unix 秒）
    #[serde(default)]
    pub last_learning_at: Option<u64>,
    /// 创建时间
    pub created_at: u64,
    /// 更新时间
    pub updated_at: u64,
}

/// 学习记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningRecord {
    pub timestamp: u64,
    pub signal: SignalSource,
    pub field_path: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
    pub confidence: f64,
}

/// 技能执行统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillStats {
    /// 总执行次数
    pub total_executions: u64,
    /// 成功次数
    pub success_count: u64,
    /// 失败次数
    pub failure_count: u64,
    /// 平均执行耗时（毫秒）
    pub avg_duration_ms: f64,
    /// 最近执行时间
    pub last_executed_at: Option<u64>,
    /// 用户接受次数
    pub accepted_count: u64,
    /// 用户忽略次数
    pub ignored_count: u64,
}

impl SkillState {
    pub fn new(skill_id: SkillId) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            skill_id,
            learned_params: HashMap::new(),
            learning_history: Vec::new(),
            stats: SkillStats::default(),
            learning_paused: false,
            last_learning_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// 记录学习结果
    pub fn record_learning(
        &mut self,
        signal: SignalSource,
        field_path: String,
        old_value: serde_json::Value,
        new_value: serde_json::Value,
        confidence: f64,
        max_history: usize,
    ) {
        let record = LearningRecord {
            timestamp: Self::now_secs(),
            signal,
            field_path,
            old_value,
            new_value,
            confidence,
        };
        self.learning_history.push(record);
        if self.learning_history.len() > max_history {
            self.learning_history
                .drain(0..self.learning_history.len() - max_history);
        }
        self.last_learning_at = Some(Self::now_secs());
        self.updated_at = Self::now_secs();
    }

    /// 记录执行结果
    pub fn record_execution(&mut self, success: bool, duration_ms: u64) {
        self.stats.total_executions += 1;
        if success {
            self.stats.success_count += 1;
        } else {
            self.stats.failure_count += 1;
        }
        let total = self.stats.total_executions;
        self.stats.avg_duration_ms = self.stats.avg_duration_ms * (total - 1) as f64 / total as f64
            + duration_ms as f64 / total as f64;
        self.stats.last_executed_at = Some(Self::now_secs());
        self.updated_at = Self::now_secs();
    }
}
