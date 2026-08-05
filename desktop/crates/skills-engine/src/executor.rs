use crate::model::{OutputStep, QueryStep, TransformStep};
use crate::registry::SkillRegistry;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

/// 技能执行上下文
pub struct ExecutionContext {
    /// 参数（来自 MCP Tool 调用或前端）
    pub params: HashMap<String, Value>,
    /// 数据库路径
    pub db_path: String,
    /// AI 端点（用于 AI 转换/输出步骤）
    pub ai_endpoint: Option<String>,
    /// AI API Key
    pub ai_api_key: Option<String>,
    /// AI 模型名
    pub ai_model: Option<String>,
    /// 是否允许执行包含 Script 步骤的技能（默认应传 false）。
    /// Script 步骤可从技能包反序列化而来且引擎尚无 required_permissions
    /// 校验链路，调用方必须显式授权后才放行，防止恶意技能包携带脚本执行。
    pub allow_script_skills: bool,
}

/// 技能执行结果
#[derive(Debug)]
pub struct ExecutionResult {
    pub skill_id: String,
    pub output: String,
    pub content_type: OutputContentType,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum OutputContentType {
    Text,
    Markdown,
    Json,
}

/// 技能执行器
pub struct SkillExecutor;

impl SkillExecutor {
    /// 执行技能
    pub fn execute(
        registry: &mut SkillRegistry,
        skill_id: &str,
        ctx: &ExecutionContext,
    ) -> ExecutionResult {
        let start = Instant::now();

        let package = match registry.get_package(skill_id) {
            Some(p) => p.clone(),
            None => {
                return ExecutionResult {
                    skill_id: skill_id.to_string(),
                    output: String::new(),
                    content_type: OutputContentType::Text,
                    duration_ms: start.elapsed().as_millis() as u64,
                    success: false,
                    error: Some(format!("技能未找到: {skill_id}")),
                }
            }
        };

        if !package.enabled {
            return ExecutionResult {
                skill_id: skill_id.to_string(),
                output: String::new(),
                content_type: OutputContentType::Text,
                duration_ms: start.elapsed().as_millis() as u64,
                success: false,
                error: Some(format!("技能已禁用: {skill_id}")),
            };
        }

        // 安全门禁：包含 Script 步骤的技能默认拒绝执行。
        // Script 步骤可随技能包反序列化进来，而引擎当前没有
        // required_permissions 的执行期校验链路，先做保守拦截，
        // 仅当调用方显式传入 allow_script_skills=true 时放行。
        let has_script_step = package
            .pipeline
            .transforms
            .iter()
            .any(|t| matches!(t, TransformStep::Script { .. }));
        if has_script_step && !ctx.allow_script_skills {
            log::warn!("技能 {skill_id} 包含 Script 步骤，未获得 allow_script_skills 授权，已拒绝执行");
            return ExecutionResult {
                skill_id: skill_id.to_string(),
                output: String::new(),
                content_type: OutputContentType::Text,
                duration_ms: start.elapsed().as_millis() as u64,
                success: false,
                error: Some(format!("技能包含未授权的 Script 步骤，已拒绝执行: {skill_id}")),
            };
        }

        // Step 1: Query
        let query_result = match Self::execute_query(&package.pipeline.query, ctx) {
            Ok(data) => data,
            Err(e) => {
                if let Some(state) = registry.get_state_mut(skill_id) {
                    state.record_execution(false, start.elapsed().as_millis() as u64);
                }
                return ExecutionResult {
                    skill_id: skill_id.to_string(),
                    output: String::new(),
                    content_type: OutputContentType::Text,
                    duration_ms: start.elapsed().as_millis() as u64,
                    success: false,
                    error: Some(format!("查询失败: {e}")),
                };
            }
        };

        // Step 2: Transforms
        let mut data = query_result;
        for transform in &package.pipeline.transforms {
            data = match Self::execute_transform(transform, &data, ctx) {
                Ok(d) => d,
                Err(e) => {
                    if let Some(state) = registry.get_state_mut(skill_id) {
                        state.record_execution(false, start.elapsed().as_millis() as u64);
                    }
                    return ExecutionResult {
                        skill_id: skill_id.to_string(),
                        output: String::new(),
                        content_type: OutputContentType::Text,
                        duration_ms: start.elapsed().as_millis() as u64,
                        success: false,
                        error: Some(format!("转换失败: {e}")),
                    };
                }
            };
        }

        // Step 3: Output
        let (output, content_type) =
            match Self::execute_output(&package.pipeline.output, &data, ctx) {
                Ok(result) => result,
                Err(e) => {
                    if let Some(state) = registry.get_state_mut(skill_id) {
                        state.record_execution(false, start.elapsed().as_millis() as u64);
                    }
                    return ExecutionResult {
                        skill_id: skill_id.to_string(),
                        output: String::new(),
                        content_type: OutputContentType::Text,
                        duration_ms: start.elapsed().as_millis() as u64,
                        success: false,
                        error: Some(format!("输出失败: {e}")),
                    };
                }
            };

        let duration_ms = start.elapsed().as_millis() as u64;
        if let Some(state) = registry.get_state_mut(skill_id) {
            state.record_execution(true, duration_ms);
        }

        ExecutionResult {
            skill_id: skill_id.to_string(),
            output,
            content_type,
            duration_ms,
            success: true,
            error: None,
        }
    }

    fn execute_query(query: &QueryStep, ctx: &ExecutionContext) -> Result<Value> {
        match query {
            QueryStep::Timeline { date, limit } => {
                let date = Self::resolve_template(date, &ctx.params);
                let limit = limit.map(|l| l.to_string()).unwrap_or_default();
                Ok(serde_json::json!({
                    "type": "timeline",
                    "date": date,
                    "limit": limit,
                    "status": "query_prepared"
                }))
            }
            QueryStep::DailyStats { date } => {
                let date = Self::resolve_template(date, &ctx.params);
                Ok(serde_json::json!({
                    "type": "daily_stats",
                    "date": date,
                    "status": "query_prepared"
                }))
            }
            QueryStep::SearchActivities {
                query: q,
                date_from,
                date_to,
                limit,
            } => {
                let q = Self::resolve_template(q, &ctx.params);
                Ok(serde_json::json!({
                    "type": "search_activities",
                    "query": q,
                    "date_from": date_from,
                    "date_to": date_to,
                    "limit": limit,
                    "status": "query_prepared"
                }))
            }
            QueryStep::WorkSessions { date } => {
                let date = Self::resolve_template(date, &ctx.params);
                Ok(serde_json::json!({
                    "type": "work_sessions",
                    "date": date,
                    "status": "query_prepared"
                }))
            }
            QueryStep::Report { date, locale } => {
                let date = Self::resolve_template(date, &ctx.params);
                Ok(serde_json::json!({
                    "type": "report",
                    "date": date,
                    "locale": locale,
                    "status": "query_prepared"
                }))
            }
            QueryStep::AnalyzeIntents { date } => {
                let date = Self::resolve_template(date, &ctx.params);
                Ok(serde_json::json!({
                    "type": "analyze_intents",
                    "date": date,
                    "status": "query_prepared"
                }))
            }
            QueryStep::DeviceStatus {} => Ok(serde_json::json!({
                "type": "device_status",
                "status": "query_prepared"
            })),
            QueryStep::Custom { params } => {
                let resolved: HashMap<String, Value> = params
                    .iter()
                    .map(|(k, v)| {
                        if let Value::String(s) = v {
                            (
                                k.clone(),
                                Value::String(Self::resolve_template(s, &ctx.params)),
                            )
                        } else {
                            (k.clone(), v.clone())
                        }
                    })
                    .collect();
                Ok(serde_json::json!({
                    "type": "custom",
                    "params": resolved,
                    "status": "query_prepared"
                }))
            }
        }
    }

    fn execute_transform(
        transform: &TransformStep,
        data: &Value,
        _ctx: &ExecutionContext,
    ) -> Result<Value> {
        match transform {
            TransformStep::Filter {
                field: _field,
                operator: _,
                value: _,
            } => {
                // 在实际实现中，这里会根据 field/operator/value 过滤 data
                Ok(data.clone())
            }
            TransformStep::Sort { field: _, order: _ } => Ok(data.clone()),
            TransformStep::Aggregate {
                group_by: _,
                metric: _,
            } => Ok(data.clone()),
            TransformStep::Limit { count: _ } => Ok(data.clone()),
            TransformStep::AiTransform {
                prompt_template,
                model,
            } => {
                // AI 转换：构建 prompt，调用 AI
                // 当前阶段返回结构化的占位数据
                Ok(serde_json::json!({
                    "ai_transform": true,
                    "prompt_template": prompt_template,
                    "model": model,
                    "input_data": data,
                    "status": "ai_transform_prepared"
                }))
            }
            TransformStep::Script {
                language: _,
                code: _,
            } => Ok(data.clone()),
        }
    }

    fn execute_output(
        output: &OutputStep,
        data: &Value,
        _ctx: &ExecutionContext,
    ) -> Result<(String, OutputContentType)> {
        match output {
            OutputStep::Text { template } => {
                let rendered = Self::render_template(template, data);
                Ok((rendered, OutputContentType::Text))
            }
            OutputStep::Markdown { template } => {
                let rendered = Self::render_template(template, data);
                Ok((rendered, OutputContentType::Markdown))
            }
            OutputStep::Json { schema: _ } => {
                let json_str = serde_json::to_string_pretty(data)?;
                Ok((json_str, OutputContentType::Json))
            }
            OutputStep::AiGenerate {
                prompt_template,
                model: _model,
            } => {
                // AI 生成输出
                let rendered = Self::render_template(prompt_template, data);
                Ok((rendered, OutputContentType::Markdown))
            }
            OutputStep::SaveReport { date, locale } => Ok((
                format!("Report saved for date={date}, locale={locale:?}"),
                OutputContentType::Text,
            )),
        }
    }

    /// 模板变量替换：{{variable}} -> value
    fn resolve_template(template: &str, params: &HashMap<String, Value>) -> String {
        let mut result = template.to_string();
        for (key, value) in params {
            let placeholder = format!("{{{{{key}}}}}");
            let val_str = match value {
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            result = result.replace(&placeholder, &val_str);
        }
        result
    }

    /// 渲染模板：将 data 的字段替换到模板中
    fn render_template(template: &str, data: &Value) -> String {
        let mut result = template.to_string();
        if let Value::Object(map) = data {
            for (key, value) in map {
                let placeholder = format!("{{{{{key}}}}}");
                let val_str = match value {
                    Value::String(s) => s.clone(),
                    _ => serde_json::to_string_pretty(value).unwrap_or_default(),
                };
                result = result.replace(&placeholder, &val_str);
            }
        }
        // 替换 {{data}} 为完整 JSON
        result = result.replace(
            "{{data}}",
            &serde_json::to_string_pretty(data).unwrap_or_default(),
        );
        result = result.replace(
            "{{ai_result}}",
            &serde_json::to_string_pretty(data).unwrap_or_default(),
        );
        result = result.replace(
            "{{weekly_data}}",
            &serde_json::to_string_pretty(data).unwrap_or_default(),
        );
        result
    }
}
