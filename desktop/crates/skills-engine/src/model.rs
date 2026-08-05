use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 技能唯一标识
pub type SkillId = String;

/// 技能包（静态逻辑，不自改）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackage {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    /// 技能分类：report / analysis / suggestion / automation
    pub category: SkillCategory,
    /// 声明式管道定义
    pub pipeline: Pipeline,
    /// 自适应配置
    #[serde(default)]
    pub adaptive: AdaptiveConfig,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 所需权限
    #[serde(default)]
    pub required_permissions: Vec<Permission>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Report,
    Analysis,
    Suggestion,
    Automation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadActivities,
    ReadReports,
    ReadStats,
    WriteReport,
    ReadConfig,
    WriteConfig,
    ExecuteAi,
    ReadSessions,
    ReadDeviceStatus,
}

/// 声明式管道：query -> transform -> output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    /// 数据查询步骤
    pub query: QueryStep,
    /// 数据转换步骤列表（按顺序执行）
    #[serde(default)]
    pub transforms: Vec<TransformStep>,
    /// 输出步骤
    pub output: OutputStep,
}

/// 查询步骤：定义数据来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryStep {
    /// 查询时间线活动
    Timeline {
        date: String,
        #[serde(default)]
        limit: Option<u32>,
    },
    /// 查询每日统计
    DailyStats { date: String },
    /// 搜索活动
    SearchActivities {
        query: String,
        #[serde(default)]
        date_from: Option<String>,
        #[serde(default)]
        date_to: Option<String>,
        #[serde(default = "default_limit")]
        limit: usize,
    },
    /// 获取工作会话
    WorkSessions { date: String },
    /// 获取报告
    Report {
        date: String,
        #[serde(default)]
        locale: Option<String>,
    },
    /// 意图分析
    AnalyzeIntents { date: String },
    /// 获取设备状态
    DeviceStatus {},
    /// 自定义参数映射
    Custom {
        params: HashMap<String, serde_json::Value>,
    },
}

fn default_limit() -> usize {
    50
}

/// 转换步骤：处理查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformStep {
    /// 过滤：只保留满足条件的数据
    Filter {
        field: String,
        operator: FilterOperator,
        value: serde_json::Value,
    },
    /// 排序
    Sort {
        field: String,
        #[serde(default = "default_descending")]
        order: SortOrder,
    },
    /// 聚合
    Aggregate {
        group_by: String,
        metric: AggregateMetric,
    },
    /// 限制数量
    Limit { count: usize },
    /// AI 处理
    AiTransform {
        prompt_template: String,
        #[serde(default)]
        model: Option<String>,
    },
    /// 自定义脚本（预留）
    Script { language: String, code: String },
}

fn default_descending() -> SortOrder {
    SortOrder::Desc
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    NotContains,
    In,
    NotIn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateMetric {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// 输出步骤：定义结果格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputStep {
    /// 文本输出
    Text { template: String },
    /// Markdown 输出
    Markdown { template: String },
    /// JSON 输出
    Json { schema: Option<serde_json::Value> },
    /// AI 生成输出
    AiGenerate {
        prompt_template: String,
        #[serde(default)]
        model: Option<String>,
    },
    /// 保存为报告
    SaveReport {
        date: String,
        locale: Option<String>,
    },
}

/// 自适应配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    /// 是否启用自适应学习
    #[serde(default)]
    pub enabled: bool,
    /// 可学习字段及其规则
    #[serde(default)]
    pub learnable_fields: Vec<LearnableField>,
    /// 最大变更幅度（0.0-1.0）
    #[serde(default = "default_max_change")]
    pub max_change_rate: f64,
    /// 冷却时间（小时）
    #[serde(default = "default_cooldown_hours")]
    pub cooldown_hours: u32,
    /// 回滚窗口（小时）
    #[serde(default = "default_rollback_window")]
    pub rollback_window_hours: u32,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            learnable_fields: Vec::new(),
            max_change_rate: default_max_change(),
            cooldown_hours: default_cooldown_hours(),
            rollback_window_hours: default_rollback_window(),
        }
    }
}

fn default_max_change() -> f64 {
    0.2
}
fn default_cooldown_hours() -> u32 {
    24
}
fn default_rollback_window() -> u32 {
    72
}

/// 可学习字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnableField {
    /// 字段路径（JSON path）
    pub field_path: String,
    /// 学习策略
    pub strategy: LearningStrategy,
    /// 学习信号来源
    #[serde(default)]
    pub signal_sources: Vec<SignalSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LearningStrategy {
    /// 简单频率统计
    Frequency,
    /// 加权平均
    WeightedAverage,
    /// 阈值调整
    ThresholdTuning,
    /// 排序偏好学习
    RankPreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    /// 用户接受建议
    AcceptSuggestion,
    /// 用户忽略建议
    IgnoreSuggestion,
    /// 用户手工修改日报
    ManualReportEdit,
    /// 用户手工修改分类
    ManualCategoryEdit,
    /// 用户执行 followup 操作
    FollowupAction,
}
