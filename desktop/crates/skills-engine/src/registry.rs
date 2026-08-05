use crate::model::{SkillId, SkillPackage};
use crate::state::SkillState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 技能注册表
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SkillRegistry {
    /// 已注册的技能包
    packages: HashMap<SkillId, SkillPackage>,
    /// 技能状态
    states: HashMap<SkillId, SkillState>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册技能
    pub fn register(&mut self, package: SkillPackage) {
        let id = package.id.clone();
        let state = SkillState::new(id.clone());
        self.states.insert(id.clone(), state);
        self.packages.insert(id, package);
    }

    /// 获取技能包
    pub fn get_package(&self, id: &str) -> Option<&SkillPackage> {
        self.packages.get(id)
    }

    /// 获取技能状态
    pub fn get_state(&self, id: &str) -> Option<&SkillState> {
        self.states.get(id)
    }

    /// 获取技能状态（可变）
    pub fn get_state_mut(&mut self, id: &str) -> Option<&mut SkillState> {
        self.states.get_mut(id)
    }

    /// 列出所有技能
    pub fn list_all(&self) -> Vec<&SkillPackage> {
        self.packages.values().collect()
    }

    /// 获取技能执行统计
    pub fn get_execution_stats(&self, id: &str) -> Option<&crate::state::SkillStats> {
        self.states.get(id).map(|s| &s.stats)
    }
}

/// 内置技能定义
pub fn builtin_skills() -> Vec<SkillPackage> {
    vec![
        daily_review_skill(),
        weekly_summary_skill(),
        project_time_audit_skill(),
        work_pattern_analysis_skill(),
        focus_session_advisor_skill(),
    ]
}

fn daily_review_skill() -> SkillPackage {
    use crate::model::*;
    SkillPackage {
        id: "daily_review".into(),
        name: "每日回顾".into(),
        description: "生成当日工作回顾和日报".into(),
        version: "1.0.0".into(),
        author: None,
        category: SkillCategory::Report,
        pipeline: Pipeline {
            query: QueryStep::DailyStats {
                date: "{{date}}".into(),
            },
            transforms: vec![TransformStep::AiTransform {
                prompt_template: "基于以下工作数据生成日报：{{data}}".into(),
                model: None,
            }],
            output: OutputStep::Markdown {
                template: "{{ai_result}}".into(),
            },
        },
        adaptive: AdaptiveConfig {
            enabled: true,
            learnable_fields: vec![LearnableField {
                field_path: "pipeline.transforms[0].prompt_template".into(),
                strategy: LearningStrategy::Frequency,
                signal_sources: vec![
                    SignalSource::AcceptSuggestion,
                    SignalSource::ManualReportEdit,
                ],
            }],
            ..Default::default()
        },
        enabled: true,
        required_permissions: vec![
            Permission::ReadActivities,
            Permission::ReadStats,
            Permission::ExecuteAi,
        ],
    }
}

fn weekly_summary_skill() -> SkillPackage {
    use crate::model::*;
    SkillPackage {
        id: "weekly_summary".into(),
        name: "周报总结".into(),
        description: "生成本周工作总结".into(),
        version: "1.0.0".into(),
        author: None,
        category: SkillCategory::Report,
        pipeline: Pipeline {
            query: QueryStep::Custom {
                params: HashMap::new(),
            },
            transforms: vec![],
            output: OutputStep::Markdown {
                template: "{{weekly_data}}".into(),
            },
        },
        adaptive: AdaptiveConfig::default(),
        enabled: true,
        required_permissions: vec![
            Permission::ReadActivities,
            Permission::ReadStats,
            Permission::ReadReports,
            Permission::ExecuteAi,
        ],
    }
}

fn project_time_audit_skill() -> SkillPackage {
    use crate::model::*;
    SkillPackage {
        id: "project_time_audit".into(),
        name: "项目时间审计".into(),
        description: "分析各项目时间投入".into(),
        version: "1.0.0".into(),
        author: None,
        category: SkillCategory::Analysis,
        pipeline: Pipeline {
            query: QueryStep::SearchActivities {
                query: "{{project}}".into(),
                date_from: None,
                date_to: None,
                limit: 100,
            },
            transforms: vec![TransformStep::Aggregate {
                group_by: "semantic_category".into(),
                metric: AggregateMetric::Sum,
            }],
            output: OutputStep::Json { schema: None },
        },
        adaptive: AdaptiveConfig::default(),
        enabled: true,
        required_permissions: vec![Permission::ReadActivities, Permission::ReadStats],
    }
}

fn work_pattern_analysis_skill() -> SkillPackage {
    use crate::model::*;
    SkillPackage {
        id: "work_pattern_analysis".into(),
        name: "工作模式分析".into(),
        description: "分析工作习惯和模式".into(),
        version: "1.0.0".into(),
        author: None,
        category: SkillCategory::Analysis,
        pipeline: Pipeline {
            query: QueryStep::WorkSessions {
                date: "{{date}}".into(),
            },
            transforms: vec![],
            output: OutputStep::AiGenerate {
                prompt_template: "分析以下工作会话数据，识别工作模式和改进建议：{{data}}".into(),
                model: None,
            },
        },
        adaptive: AdaptiveConfig {
            enabled: true,
            learnable_fields: vec![LearnableField {
                field_path: "pipeline.output.prompt_template".into(),
                strategy: LearningStrategy::Frequency,
                signal_sources: vec![
                    SignalSource::AcceptSuggestion,
                    SignalSource::IgnoreSuggestion,
                ],
            }],
            ..Default::default()
        },
        enabled: true,
        required_permissions: vec![
            Permission::ReadActivities,
            Permission::ReadSessions,
            Permission::ExecuteAi,
        ],
    }
}

fn focus_session_advisor_skill() -> SkillPackage {
    use crate::model::*;
    SkillPackage {
        id: "focus_session_advisor".into(),
        name: "专注建议".into(),
        description: "基于历史数据给出专注时段建议".into(),
        version: "1.0.0".into(),
        author: None,
        category: SkillCategory::Suggestion,
        pipeline: Pipeline {
            query: QueryStep::DailyStats {
                date: "{{date}}".into(),
            },
            transforms: vec![],
            output: OutputStep::AiGenerate {
                prompt_template: "基于以下工作统计数据，推荐最佳专注时段：{{data}}".into(),
                model: None,
            },
        },
        adaptive: AdaptiveConfig {
            enabled: true,
            learnable_fields: vec![LearnableField {
                field_path: "pipeline.output.prompt_template".into(),
                strategy: LearningStrategy::WeightedAverage,
                signal_sources: vec![SignalSource::FollowupAction],
            }],
            ..Default::default()
        },
        enabled: true,
        required_permissions: vec![
            Permission::ReadStats,
            Permission::ReadSessions,
            Permission::ExecuteAi,
        ],
    }
}
