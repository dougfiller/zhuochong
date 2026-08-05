//! Auto-extracted from the historical `commands.rs`. Behavior unchanged.

use crate::analysis::AppLocale;
use crate::config::{AiProvider, ModelConfig};
use crate::database::MemorySearchItem;
use crate::error::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::State;

use super::shared::collect_privacy_filters;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssistantAnswer {
    pub answer: String,
    pub references: Vec<MemorySearchItem>,
    pub used_ai: bool,
    pub model_name: Option<String>,
    pub tool_labels: Vec<String>,
    pub cards: Vec<AssistantCard>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssistantChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssistantCard {
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
}

fn assistant_empty_question_message(locale: AppLocale) -> &'static str {
    match locale {
        AppLocale::ZhCn => "请输入你想问的问题。",
        AppLocale::ZhTw => "請輸入你想問的問題。",
        AppLocale::En => "Please enter your question.",
        AppLocale::Ar => "الرجاء إدخال سؤالك.",
    }
}

fn empty_question_tool_labels() -> Vec<String> {
    Vec::new()
}

fn build_assistant_system_prompt(locale: AppLocale) -> String {
    // 基础 prompt：locale 感知的工作助手定位。
    let base = match locale {
        AppLocale::ZhCn => {
            "你是 Work Review 的工作助手。你可以回答任何问题。对于工作相关问题，你拥有工具可以查询用户的真实工作记录（活动时间线、统计、工作会话等），请优先使用工具获取准确数据后回答。对于非工作问题，直接用你的知识回答即可。请用与用户提问相同的语言回答，无论工作记录是什么语言（英文提问用英文，中文提问用中文）。先给结论再给依据，不要编造不存在的事实。"
        }
        AppLocale::ZhTw => {
            "你是 Work Review 的工作助手。你可以回答任何問題。對於工作相關問題，你擁有工具可以查詢使用者的真實工作記錄（活動時間線、統計、工作會話等），請優先使用工具獲取準確資料後回答。對於非工作問題，直接用你的知識回答即可。請用與使用者提問相同的語言回答，無論工作記錄是什麼語言（英文提問用英文，中文提問用中文）。先給結論再給依據，不要編造不存在的事實。"
        }
        AppLocale::En => {
            "You are the Work Review assistant. You can answer any question. For work-related questions, you have tools to query the user's actual work records (activity timeline, statistics, work sessions, etc.) — use them for accuracy. For non-work questions, answer directly from your knowledge. Respond in the same language as the user's question, regardless of the language of the work records (English question -> English answer, Chinese question -> Chinese answer). Lead with the conclusion, then support with evidence. Do not invent facts."
        }
        AppLocale::Ar => {
            "أنت مساعد Work Review. يمكنك الإجابة على أي سؤال. للأسئلة المتعلقة بالعمل، لديك أدوات للاستعلام عن سجلات عمل المستخدم الفعلية (الجدول الزمني للنشاط، الإحصائيات، جلسات العمل، وما إلى ذلك) — استخدمها لضمان الدقة. بالنسبة للأسئلة غير المتعلقة بالعمل، أجب مباشرة من معرفتك. قم بالرد بنفس لغة سؤال المستخدم، بغض النظر عن لغة سجلات العمل (سؤال عربي -> إجابة عربية). ابدأ بالخلاصة، ثم ادعمها بالأدلة. لا تختلق الحقائق."
        }
    };

    // 工具历史摘要声明（locale 感知）。
    // 多轮对话的 assistant 消息 content 尾部会带 `[工具：...]` 形式的机器摘要，
    // 告诉模型这是什么、不要复述给用户、`✓/↯/?/→N条` 的含义。
    // 注意：这段必须加在 build_assistant_system_prompt 里而不是 executor.rs 的
    // DEFAULT_SYSTEM_PROMPT，因为生产路径 chat_work_assistant 始终传 Some(prompt)，
    // unwrap_or(DEFAULT_...) 永远走不到（codex 二轮 review 发现的死代码 bug）。
    let tool_trace_hint = match locale {
        AppLocale::ZhCn => {
            "\n\n历史对话里出现的 `[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]` 形式的方括号片段是上一轮工具调用的机器摘要，不是用户的话：`→N条` 表示命中记忆条数，`✓` 表示工具成功执行，`↯` 表示工具失败（避免重复调用），`?` 表示旧数据状态未知，不能视为成功或失败。回答时不要向用户复述这个摘要，也不要把它当作已确认的事实——它只是工具历史轨迹提示。"
        }
        AppLocale::ZhTw => {
            "\n\n歷史對話裡出現的 `[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]` 形式的方括號片段是上一輪工具呼叫的機器摘要，不是使用者的話：`→N条` 表示命中記憶條數，`✓` 表示工具成功執行，`↯` 表示工具失敗（避免重複呼叫），`?` 表示舊資料狀態未知，不能視為成功或失敗。回答時不要向使用者複述這個摘要，也不要把它當作已確認的事實——它只是工具歷史軌跡提示。"
        }
        AppLocale::En => {
            "\n\nBracketed snippets like `[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]` that appear in conversation history are machine-generated summaries of the previous turn's tool calls, not the user's words: `→N条` means N memory records matched, `✓` means the tool executed successfully, `↯` means it failed (avoid retrying it), and `?` means legacy data has an unknown status and must not be treated as success or failure. Do not repeat this summary back to the user, and do not treat it as a confirmed fact — it is only a tool-history hint."
        }
        AppLocale::Ar => {
            "\n\nالمقتطفات بين الأقواس مثل `[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]` التي تظهر في سجل المحادثة هي ملخصات آلية لاستدعاءات الأدوات في الدور السابق، وليست كلام المستخدم: `→N条` تعني N سجلات ذاكرة مطابقة، و`✓` تعني أن الأداة نُفِّذت بنجاح، و`↯` تعني أنها فشلت (تجنّب إعادة المحاولة)، وتعني `?` أن حالة البيانات القديمة غير معروفة، ولا يجوز اعتبارها نجاحًا أو فشلًا. لا تكرر هذا الملخص للمستخدم، ولا تعتبره حقيقة مؤكدة — إنه مجرد تلميح عن سجل الأدوات."
        }
    };

    format!("{base}{tool_trace_hint}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum AssistantQuestionKind {
    StageSummary,
    OutcomeRecap,
    ProcessRecap,
    EvidenceQuery,
    TimeStat,
    Comparison,
    Listing,
    Freeform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssistantReasoningMode {
    Basic,
    AiEnhanced,
}

fn build_history_context(history: &[AssistantChatMessage]) -> String {
    history
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| format!("{}: {}", message.role, message.content.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_short_follow_up_question(question: &str) -> bool {
    let trimmed = question.trim();
    let normalized = trimmed.to_lowercase();

    trimmed.chars().count() <= 18
        && [
            "继续",
            "展开",
            "细说",
            "详细",
            "具体",
            "接着",
            "那",
            "这个",
            "这里",
            "这个结论",
            "说说",
            "依据",
        ]
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

fn build_question_analysis_context(question: &str, history: &[AssistantChatMessage]) -> String {
    let trimmed = question.trim();
    if history.is_empty() {
        return trimmed.to_lowercase();
    }

    let should_expand = trimmed.chars().count() <= 18
        || [
            "这个",
            "这个结论",
            "这里",
            "这些",
            "它",
            "上面",
            "刚才",
            "继续",
            "展开",
            "依据",
        ]
        .iter()
        .any(|pattern| trimmed.contains(pattern));

    if !should_expand {
        return trimmed.to_lowercase();
    }

    let mut context = build_history_context(history);
    if !context.is_empty() {
        context.push('\n');
    }
    context.push_str(trimmed);
    context.to_lowercase()
}

fn detect_question_kind_from_text(text: &str) -> AssistantQuestionKind {
    let context = text.trim().to_lowercase();

    if context.is_empty() {
        return AssistantQuestionKind::StageSummary;
    }

    let time_stat_patterns = ["花了多少时间", "多少时间", "总时长", "时间分布", "时间占比"];
    if time_stat_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::TimeStat;
    }

    let comparison_patterns = ["对比", "比较", "和上周", "相比", "比上周", "变化", "差异"];
    if comparison_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::Comparison;
    }

    let listing_patterns = ["列出", "列举", "所有", "全部", "哪些", "清单"];
    if listing_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::Listing;
    }

    let evidence_patterns = [
        "依据",
        "证据",
        "怎么得出",
        "怎么判断",
        "为什么这么说",
        "哪些记录",
        "哪条记录",
        "从哪里看",
        "原文",
    ];
    if evidence_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::EvidenceQuery;
    }

    let process_patterns = [
        "过程",
        "怎么推进",
        "时间花在哪",
        "花在哪",
        "节奏",
        "session",
        "工作段",
        "时段",
        "时间线",
        "切换",
        "过程复盘",
    ];
    if process_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::ProcessRecap;
    }

    let outcome_patterns = [
        "结果",
        "产出",
        "完成了什么",
        "推进到哪",
        "进展",
        "交付",
        "没收口",
        "待办",
        "下一步",
        "后续",
        "风险",
        "阻塞",
    ];
    if outcome_patterns
        .iter()
        .any(|pattern| context.contains(pattern))
    {
        return AssistantQuestionKind::OutcomeRecap;
    }

    AssistantQuestionKind::StageSummary
}

fn last_user_question_kind(history: &[AssistantChatMessage]) -> Option<AssistantQuestionKind> {
    history
        .iter()
        .rev()
        .find(|message| message.role == "user" && !message.content.trim().is_empty())
        .map(|message| detect_question_kind_from_text(&message.content))
}

fn infer_question_kind_from_assistant_reply(
    history: &[AssistantChatMessage],
) -> Option<AssistantQuestionKind> {
    let content = history
        .iter()
        .rev()
        .find(|message| message.role == "assistant" && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_lowercase())?;

    let mut best_kind = AssistantQuestionKind::StageSummary;
    let mut best_score = 0i32;

    let candidates: Vec<(AssistantQuestionKind, &[&str])> = vec![
        (
            AssistantQuestionKind::EvidenceQuery,
            &[
                "## 依据补充",
                "依据",
                "记录",
                "原始记录",
                "证据",
                "哪条记录",
            ],
        ),
        (
            AssistantQuestionKind::ProcessRecap,
            &[
                "## 过程分析",
                "session",
                "工作段",
                "时间花在",
                "推进片段",
                "切换",
            ],
        ),
        (
            AssistantQuestionKind::OutcomeRecap,
            &["待办", "风险", "交付", "结果概览", "收口", "下一步"],
        ),
        (
            AssistantQuestionKind::StageSummary,
            &["结论", "主线", "阶段", "主要做了什么", "工作重心"],
        ),
        (
            AssistantQuestionKind::TimeStat,
            &["## 时间统计", "时长", "时间分布", "占比", "花了多少时间"],
        ),
        (
            AssistantQuestionKind::Comparison,
            &["## 对比分析", "对比", "比较", "变化", "差异", "相比"],
        ),
        (
            AssistantQuestionKind::Listing,
            &["## 清单", "列举", "列出", "所有", "全部", "清单"],
        ),
    ];

    for (kind, patterns) in candidates {
        let score = patterns
            .iter()
            .map(|pattern| {
                if content.contains(pattern) {
                    if pattern.starts_with("## ") {
                        3
                    } else {
                        1
                    }
                } else {
                    0
                }
            })
            .sum::<i32>();

        if score > best_score {
            best_score = score;
            best_kind = kind;
        }
    }

    if best_score > 0 {
        Some(best_kind)
    } else {
        None
    }
}

fn detect_assistant_question_kind_with_mode(
    question: &str,
    history: &[AssistantChatMessage],
    mode: AssistantReasoningMode,
) -> AssistantQuestionKind {
    let trimmed = question.trim();
    let current_kind = detect_question_kind_from_text(trimmed);

    if current_kind == AssistantQuestionKind::EvidenceQuery
        || current_kind == AssistantQuestionKind::TimeStat
        || current_kind == AssistantQuestionKind::Comparison
        || current_kind == AssistantQuestionKind::Listing
    {
        return current_kind;
    }

    if is_short_follow_up_question(trimmed) {
        if mode == AssistantReasoningMode::AiEnhanced {
            if let Some(assistant_kind) = infer_question_kind_from_assistant_reply(history) {
                if assistant_kind != AssistantQuestionKind::StageSummary {
                    return assistant_kind;
                }
            }
        }

        if let Some(previous_kind) = last_user_question_kind(history) {
            return previous_kind;
        }
    }

    let context = build_question_analysis_context(question, history);
    let contextual_kind = detect_question_kind_from_text(&context);
    if contextual_kind != AssistantQuestionKind::StageSummary {
        return contextual_kind;
    }

    current_kind
}

#[allow(dead_code)]
fn detect_assistant_question_kind(
    question: &str,
    history: &[AssistantChatMessage],
) -> AssistantQuestionKind {
    detect_assistant_question_kind_with_mode(question, history, AssistantReasoningMode::Basic)
}

#[allow(dead_code)]
fn push_markdown_section(answer: &mut String, title: &str, lines: Vec<String>, empty_text: &str) {
    if lines.is_empty() && empty_text.is_empty() {
        return;
    }

    answer.push_str(title);
    answer.push_str("\n\n");

    if lines.is_empty() {
        answer.push_str(empty_text);
        answer.push_str("\n\n");
        return;
    }

    for line in lines {
        if line.starts_with("- ") || line.starts_with("> ") {
            answer.push_str(&line);
        } else {
            answer.push_str("- ");
            answer.push_str(&line);
        }
        answer.push('\n');
    }
    answer.push('\n');
}

pub(crate) async fn generate_text_answer_with_model(
    model_config: &ModelConfig,
    system_prompt: &str,
    prompt: &str,
) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Unknown(e.to_string()))?;

    match model_config.provider {
        AiProvider::Ollama => {
            let ollama_base = model_config.endpoint.trim().trim_end_matches('/');
            let ollama_url = if ollama_base.ends_with("/api/chat") {
                ollama_base.to_string()
            } else {
                format!("{ollama_base}/api/chat")
            };
            let response = client
                .post(&ollama_url)
                .json(&serde_json::json!({
                    "model": model_config.model,
                    "messages": [
                        {
                            "role": "system",
                            "content": system_prompt
                        },
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ],
                    "stream": false
                }))
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(AppError::Analysis(format!(
                    "Ollama 记忆问答失败: {}",
                    response.status()
                )));
            }

            let result: serde_json::Value = response.json().await?;
            let answer = result["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if answer.is_empty() {
                return Err(AppError::Analysis("Ollama 返回空内容".to_string()));
            }
            Ok(answer)
        }
        AiProvider::Claude => {
            let api_key = model_config.api_key.as_deref().unwrap_or("");
            if api_key.is_empty() {
                return Err(AppError::Analysis("Claude API Key 未配置".to_string()));
            }

            let claude_base = model_config.endpoint.trim().trim_end_matches('/');
            let claude_url = if claude_base.ends_with("/messages") {
                claude_base.to_string()
            } else {
                format!("{claude_base}/messages")
            };
            let response = client
                .post(&claude_url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": model_config.model,
                    "max_tokens": 1600,
                    "system": system_prompt,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ]
                }))
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await.unwrap_or_default();
                return Err(AppError::Analysis(format!(
                    "Claude 记忆问答失败: {error_text}"
                )));
            }

            let result: serde_json::Value = response.json().await?;
            let answer = result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if answer.is_empty() {
                return Err(AppError::Analysis("Claude 返回空内容".to_string()));
            }
            Ok(answer)
        }
        AiProvider::Gemini => {
            let api_key = model_config.api_key.as_deref().unwrap_or("");
            if api_key.is_empty() {
                return Err(AppError::Analysis("Gemini API Key 未配置".to_string()));
            }

            let gemini_base = model_config.endpoint.trim().trim_end_matches('/');
            // Key 走请求头而非 URL query，避免进代理日志/Referer（与 ai.rs / model.rs 一致）
            let gemini_url = format!(
                "{}/models/{}:generateContent",
                gemini_base, model_config.model
            );
            let response = client
                .post(&gemini_url)
                .header("x-goog-api-key", api_key)
                .json(&serde_json::json!({
                    // system 指令走 systemInstruction 字段（而非拼进 user content），
                    // 保持系统指令优先级，降低外部文本注入的影响面
                    "contents": [{
                        "parts": [{ "text": prompt }]
                    }],
                    "systemInstruction": {
                        "parts": [{ "text": system_prompt }]
                    },
                    "generationConfig": {
                        "temperature": 0.2,
                        "maxOutputTokens": 1600
                    }
                }))
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await.unwrap_or_default();
                return Err(AppError::Analysis(format!(
                    "Gemini 记忆问答失败: {error_text}"
                )));
            }

            let result: serde_json::Value = response.json().await?;
            let answer = result["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if answer.is_empty() {
                return Err(AppError::Analysis("Gemini 返回空内容".to_string()));
            }
            Ok(answer)
        }
        _ => {
            let endpoint = model_config.endpoint.trim().trim_end_matches('/');
            let url = if endpoint.ends_with("/chat/completions") {
                endpoint.to_string()
            } else {
                format!("{endpoint}/chat/completions")
            };
            let mut request = client.post(&url).json(&serde_json::json!({
                "model": model_config.model,
                "messages": [
                    {
                        "role": "system",
                        "content": system_prompt
                    },
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "max_tokens": 1600,
                "temperature": 0.2
            }));

            if let Some(api_key) = &model_config.api_key {
                if !api_key.is_empty() {
                    request = request.header("Authorization", format!("Bearer {api_key}"));
                }
            }

            let response = request.send().await?;

            if !response.status().is_success() {
                let error_text = response.text().await.unwrap_or_default();
                return Err(AppError::Analysis(format!(
                    "OpenAI 兼容记忆问答失败: {error_text}"
                )));
            }

            let result: serde_json::Value = response.json().await?;
            let answer = result["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if answer.is_empty() {
                return Err(AppError::Analysis("模型返回空内容".to_string()));
            }
            Ok(answer)
        }
    }
}

fn agent_event_delivery_error(message: impl Into<String>) -> AppError {
    AppError::Analysis(format!(
        "助手事件投递失败，已停止后续处理: {}",
        message.into()
    ))
}

fn send_channel_event(
    channel: &tauri::ipc::Channel<crate::agent::StreamEvent>,
    event: crate::agent::StreamEvent,
) -> Result<(), AppError> {
    channel
        .send(event)
        .map_err(|error| agent_event_delivery_error(error.to_string()))
}

/// 把 Agent 内部事件转发到实际输出通道，并向控制事件回传真实投递结果。
async fn bridge_agent_events<F>(
    mut rx: tokio::sync::mpsc::Receiver<crate::agent::events::StreamEventEnvelope>,
    mut send: F,
) -> Result<(), String>
where
    F: FnMut(crate::agent::StreamEvent) -> Result<(), String>,
{
    while let Some(envelope) = rx.recv().await {
        let send_result = send(envelope.event);
        let should_stop = send_result.is_err();

        if let Some(delivery_ack) = envelope.delivery_ack {
            let _ = delivery_ack.send(send_result.clone());
        }

        if should_stop {
            return send_result;
        }
    }

    Ok(())
}

// ══════════════════════════════════════════════════════════
// 助手运行时桥接：停止 / 确认 / 行动 / 实时上下文
// ══════════════════════════════════════════════════════════

/// 在途请求的停止信号（request_id → watch sender）。前端"停止"按钮触发
/// `cancel_assistant_request` 置位，executor 在安全点收束。
static ASSISTANT_CANCEL_SENDERS: once_cell::sync::Lazy<
    Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>,
> = once_cell::sync::Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

type PendingAssistantConfirmation =
    (std::time::Instant, tokio::sync::oneshot::Sender<bool>);
type AssistantConfirmationMap =
    std::collections::HashMap<String, PendingAssistantConfirmation>;

/// 待确认的行动（confirm_id → (创建时间, oneshot sender)）。
/// 用户在确认卡片上点击后经 `confirm_assistant_action` 回传。
static ASSISTANT_CONFIRMATIONS: once_cell::sync::Lazy<Mutex<AssistantConfirmationMap>> =
    once_cell::sync::Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// 请求结束时移除停止信号注册（无论正常返回还是错误）。
struct CancelRegistrationGuard {
    request_id: Option<String>,
}

impl Drop for CancelRegistrationGuard {
    fn drop(&mut self) {
        if let Some(id) = self.request_id.take() {
            if let Ok(mut map) = ASSISTANT_CANCEL_SENDERS.lock() {
                map.remove(&id);
            }
        }
    }
}

/// 前端"停止"按钮：置位在途请求的取消信号。
#[tauri::command]
pub async fn cancel_assistant_request(request_id: String) -> Result<(), AppError> {
    let map = ASSISTANT_CANCEL_SENDERS
        .lock()
        .map_err(|e| AppError::Unknown(e.to_string()))?;
    if let Some(tx) = map.get(&request_id) {
        let _ = tx.send(true);
    }
    Ok(())
}

/// 前端确认卡片：回传用户对某个行动的批准/拒绝。
#[tauri::command]
pub async fn confirm_assistant_action(confirm_id: String, approved: bool) -> Result<(), AppError> {
    let entry = {
        let mut map = ASSISTANT_CONFIRMATIONS
            .lock()
            .map_err(|e| AppError::Unknown(e.to_string()))?;
        map.remove(&confirm_id)
    };
    if let Some((_, tx)) = entry {
        // executor 侧超时后 rx 已 drop，send 失败是正常情况
        let _ = tx.send(approved);
    }
    Ok(())
}

/// 确认桥：注册 oneshot 并返回等待 Future。插入时顺带清理超过 1 小时的陈旧条目。
fn build_confirm_bridge() -> crate::agent::ConfirmBridge {
    crate::agent::ConfirmBridge {
        wait: std::sync::Arc::new(|confirm_id: String| {
            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            if let Ok(mut map) = ASSISTANT_CONFIRMATIONS.lock() {
                map.retain(|_, (created, _)| created.elapsed().as_secs() < 3600);
                map.insert(confirm_id, (std::time::Instant::now(), tx));
            }
            Box::pin(async move {
                match rx.await {
                    Ok(true) => crate::agent::ConfirmDecision::Approved,
                    Ok(false) => crate::agent::ConfirmDecision::Denied,
                    Err(_) => crate::agent::ConfirmDecision::TimedOut,
                }
            })
        }),
    }
}

/// 实时上下文：当前前台窗口（经隐私过滤）+ 今日概况。
/// 注入 system prompt，并作为 get_current_context 工具的数据源。
fn build_realtime_context_text(state_arc: &Arc<Mutex<AppState>>) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let active_window = crate::monitor::get_active_window_fast().ok();

    let mut parts: Vec<String> = Vec::new();

    if let Ok(s) = state_arc.lock() {
        // 前台窗口（尊重隐私规则：Skip 不注入，Anonymize 脱敏标题并去 URL）
        if let Some(w) = active_window.as_ref() {
            match s.privacy_filter.check_privacy_full(
                &w.app_name,
                &w.window_title,
                w.browser_url.as_deref(),
            ) {
                crate::privacy::PrivacyAction::Skip => {}
                crate::privacy::PrivacyAction::Anonymize => {
                    parts.push(format!("当前前台应用: {} — [内容已脱敏]", w.app_name));
                }
                crate::privacy::PrivacyAction::Record => {
                    let title: String = w.window_title.chars().take(80).collect();
                    parts.push(format!("当前前台应用: {} — {}", w.app_name, title));
                }
            }
        }

        // 今日概况：从今日时间线聚合（轻量，最多 300 行）
        if let Ok(activities) = s.database.get_timeline(&today, Some(300), None) {
            let (ignored_apps, excluded_domains) = collect_privacy_filters(&s);
            let activities = super::filter_activities_by_privacy(
                activities,
                &ignored_apps,
                &excluded_domains,
            );
            if !activities.is_empty() {
                let total: i64 = activities.iter().map(|a| a.duration).sum();
                let mut app_totals: std::collections::HashMap<String, i64> =
                    std::collections::HashMap::new();
                for a in &activities {
                    *app_totals.entry(a.app_name.clone()).or_insert(0) += a.duration;
                }
                let mut ranked: Vec<(String, i64)> = app_totals.into_iter().collect();
                ranked.sort_by_key(|item| std::cmp::Reverse(item.1));
                let top: Vec<String> = ranked
                    .iter()
                    .take(3)
                    .map(|(name, secs)| format!("{}({}分)", name, secs / 60))
                    .collect();
                parts.push(format!(
                    "今日已记录约 {} 分钟，Top应用: {}",
                    total / 60,
                    top.join("、")
                ));
            }
        }
    }

    if parts.is_empty() {
        "当前无实时上下文（可能刚启动或今日暂无记录）。".to_string()
    } else {
        parts.join("；")
    }
}

/// 行动桥：把 Agent 的写操作落到真实的应用状态/配置/事件上。
/// 所有操作已经过 executor 的用户确认流程。
fn build_action_bridge(
    app: tauri::AppHandle,
    state_arc: Arc<Mutex<AppState>>,
    locale: Option<String>,
) -> crate::agent::ActionBridge {
    crate::agent::ActionBridge {
        run: std::sync::Arc::new(move |action| {
            let app = app.clone();
            let state_arc = state_arc.clone();
            let locale = locale.clone();
            Box::pin(async move { execute_assistant_action(action, app, state_arc, locale).await })
        }),
    }
}

async fn execute_assistant_action(
    action: crate::agent::AssistantAction,
    app: tauri::AppHandle,
    state_arc: Arc<Mutex<AppState>>,
    locale: Option<String>,
) -> Result<String, String> {
    use crate::agent::AssistantAction;

    match action {
        AssistantAction::CreateTodo { text } => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let next_config = {
                let s = state_arc.lock().map_err(|e| e.to_string())?;
                let mut next = s.config.clone();
                next.avatar_followups.push(crate::config::AvatarFollowupItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: text.clone(),
                    date: today,
                    source_app: "工作助手".to_string(),
                    source_title: "助手对话".to_string(),
                    project_key: String::new(),
                    created_at: chrono::Utc::now().timestamp(),
                    status: "open".to_string(),
                });
                next
            };
            super::persist_app_config(next_config, app, &state_arc)
                .map_err(|e| format!("保存待办失败: {e}"))?;
            Ok(format!("已创建待办：{text}"))
        }

        AssistantAction::SetAppCategory { app_name, category } => {
            let next_config = {
                let s = state_arc.lock().map_err(|e| e.to_string())?;
                let mut next = s.config.clone();
                super::category::upsert_app_category_rule(&mut next, &app_name, &category);
                next
            };
            super::persist_app_config(next_config, app, &state_arc)
                .map_err(|e| format!("保存分类规则失败: {e}"))?;
            let updated = {
                let s = state_arc.lock().map_err(|e| e.to_string())?;
                super::category::reclassify_app_history_in_state(&s, &app_name, &category)
                    .map_err(|e| format!("同步历史记录失败: {e}"))?
            };
            Ok(format!(
                "已把「{app_name}」的分类改为「{category}」，同步更新 {updated} 条历史记录"
            ))
        }

        AssistantAction::PauseRecording => {
            {
                let mut s = state_arc.lock().map_err(|e| e.to_string())?;
                s.is_paused = true;
            }
            crate::emit_recording_state_changed(&app);
            Ok("已暂停屏幕活动记录".to_string())
        }

        AssistantAction::ResumeRecording => {
            {
                let mut s = state_arc.lock().map_err(|e| e.to_string())?;
                s.is_recording = true;
                s.is_paused = false;
            }
            crate::emit_recording_state_changed(&app);
            Ok("已恢复屏幕活动记录".to_string())
        }

        AssistantAction::OpenTimeline { date } => {
            use tauri::Emitter;
            let date = if date.is_empty() {
                chrono::Local::now().format("%Y-%m-%d").to_string()
            } else {
                date
            };
            app.emit("avatar-open-timeline", serde_json::json!({ "date": date }))
                .map_err(|e| format!("打开时间线失败: {e}"))?;
            Ok(format!("已打开 {date} 的时间线"))
        }

        AssistantAction::GenerateDailyReport { date, force } => {
            // 与 generate_report 命令共用防并发标志
            {
                let mut s = state_arc.lock().map_err(|e| e.to_string())?;
                if s.generating_report {
                    return Err("日报正在生成中，请稍候".to_string());
                }
                s.generating_report = true;
            }
            let result = super::report::generate_report_inner(
                date.clone(),
                Some(force),
                locale,
                &app,
                &state_arc,
            )
            .await;
            if let Ok(mut s) = state_arc.lock() {
                s.generating_report = false;
            }
            match result {
                Ok(_) => Ok(format!(
                    "已生成 {date} 的日报。可调用 get_daily_report 读取内容，或让用户到日报页查看。"
                )),
                Err(e) => Err(format!("生成日报失败: {e}")),
            }
        }
    }
}

/// 统一工作助手（Stage 6: 已接入 Agent Orchestrator）
///
/// 接口签名保持不变，内部实现替换为 Agentic 架构：
/// - 简单查询 → FastPath（规则 + 模板）
/// - 复杂查询 → AgentPath（LLM 自主决策 + 多轮工具调用）
/// - 无模型   → FallbackPath（纯模板回答）
#[tauri::command]
#[allow(unused_variables, clippy::too_many_arguments)] // date_from/date_to 为接口预留，Agent 当前从问题自行推断时间范围
pub async fn chat_work_assistant(
    question: String,
    history: Option<Vec<AssistantChatMessage>>,
    model_config: Option<ModelConfig>,
    locale: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    request_id: Option<String>,
    on_event: tauri::ipc::Channel<crate::agent::StreamEvent>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<AssistantAnswer, AppError> {
    let trimmed_question = question.trim().to_string();
    let history = history.unwrap_or_default();
    let assistant_locale = AppLocale::from_option(locale.as_deref());

    if trimmed_question.is_empty() {
        let answer = assistant_empty_question_message(assistant_locale).to_string();
        let tool_labels = empty_question_tool_labels();
        // 空问题也推一个 Done，保持事件流完整（前端可统一收尾）。
        send_channel_event(
            &on_event,
            crate::agent::StreamEvent::Done {
                answer: answer.clone(),
                references: vec![],
                tool_labels: tool_labels.clone(),
            },
        )?;
        return Ok(AssistantAnswer {
            answer,
            references: Vec::new(),
            used_ai: false,
            model_name: None,
            tool_labels,
            cards: Vec::new(),
        });
    }

    // 将前端历史转为 Agent 内部的 Message 格式（保留 role）
    let agent_history: Vec<crate::agent::Message> = history
        .iter()
        .map(|m| {
            if m.role == "assistant" {
                crate::agent::Message::assistant(&m.content)
            } else {
                crate::agent::Message::user(&m.content)
            }
        })
        .collect();

    // 从 AppState 中 clone Database + 收集隐私过滤器（Arc 引用计数 +1，可跨 await）
    let (database, ignored_apps, excluded_domains, web_tools, avatar_followups) = {
        let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        let (ignored_apps, excluded_domains) = collect_privacy_filters(&s);
        // 联网工具配置：仅在用户显式开启时传入（隐私默认关）
        let web_tools = if s.config.assistant_web_access_enabled {
            Some(crate::agent::WebToolsConfig {
                provider: s.config.assistant_search_provider.clone(),
                api_key: s.config.assistant_search_api_key.clone(),
            })
        } else {
            None
        };
        (
            s.database.clone(),
            ignored_apps,
            excluded_domains,
            web_tools,
            s.config.avatar_followups.clone(),
        )
    };

    // 停止信号注册（request_id 由前端生成；guard 确保请求结束后清理）
    let cancel_rx = request_id.as_ref().map(|id| {
        let (tx, rx) = tokio::sync::watch::channel(false);
        if let Ok(mut map) = ASSISTANT_CANCEL_SENDERS.lock() {
            map.insert(id.clone(), tx);
        }
        rx
    });
    let _cancel_guard = CancelRegistrationGuard {
        request_id: request_id.clone(),
    };

    // 助手运行时能力：行动桥 + 确认桥 + 实时上下文提供者 + 语义检索桥
    let state_arc: Arc<Mutex<AppState>> = state.inner().clone();
    let context_state = state_arc.clone();
    let semantic_enabled = {
        let s = state_arc.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        s.config.memory_semantic_enabled
    };
    let semantic_search = if semantic_enabled {
        let semantic_state = state_arc.clone();
        Some(std::sync::Arc::new(move |query: String, limit: usize| {
            let semantic_state = semantic_state.clone();
            Box::pin(async move {
                let hits =
                    super::semantic_memory::search_semantic_memory_inner(&semantic_state, &query, limit)
                        .await
                        .map_err(|e| e.to_string())?;
                if hits.is_empty() {
                    return Ok(format!("「{query}」没有检索到相关屏幕记忆。"));
                }
                let lines: Vec<String> = hits
                    .iter()
                    .map(|h| {
                        let url = h
                            .browser_url
                            .as_deref()
                            .map(|u| format!("\n  {u}"))
                            .unwrap_or_default();
                        format!(
                            "- [{}] {} — {}{url}\n  摘要: {}",
                            h.date, h.app_name, h.title, h.excerpt
                        )
                    })
                    .collect();
                Ok(lines.join("\n"))
            }) as crate::agent::tools::ActionFuture
        })
            as std::sync::Arc<
                dyn Fn(String, usize) -> crate::agent::tools::ActionFuture + Send + Sync,
            >)
    } else {
        None
    };
    let runtime = crate::agent::AssistantRuntime {
        avatar_followups,
        actions: Some(build_action_bridge(
            app.clone(),
            state_arc.clone(),
            locale.clone(),
        )),
        confirm: Some(build_confirm_bridge()),
        current_context: Some(std::sync::Arc::new(move || {
            build_realtime_context_text(&context_state)
        })),
        semantic_search,
        cancel: cancel_rx,
    };

    // 流式桥接：Token 可丢；控制事件必须等 Tauri Channel 实际发送成功后再确认。
    let (tx, rx) = crate::agent::StreamEventSender::channel(64);
    let on_event_clone = on_event.clone();
    let bridge = tauri::async_runtime::spawn(async move {
        bridge_agent_events(rx, move |event| {
            on_event_clone
                .send(event)
                .map_err(|error| error.to_string())
        })
        .await
    });

    // Stage 6: 完整 Orchestrator 集成
    // 使用 locale 感知的系统提示词，确保繁体/英文用户得到对应语言的回答；
    // 追加实时上下文（前台窗口 + 今日概况），让模型能回答"我现在在干嘛"类问题。
    let realtime_context = build_realtime_context_text(&state_arc);
    let system_prompt = format!(
        "{}\n\n[实时上下文] {realtime_context}\n（此信息由系统在请求开始时注入；会话中途需要最新状态时调用 get_current_context 工具。）",
        build_assistant_system_prompt(assistant_locale)
    );
    let result = crate::agent::Orchestrator::handle(
        &trimmed_question,
        model_config.as_ref(),
        &database,
        &agent_history,
        Some(&system_prompt),
        &ignored_apps,
        &excluded_domains,
        web_tools,
        runtime,
        Some(tx),
    )
    .await;

    // 等桥接任务把剩余事件发完（tx 在 handle 内 drop 后 rx.recv() 返回 None）。
    let bridge_result = bridge
        .await
        .map_err(|error| AppError::Unknown(format!("助手事件桥接任务异常: {error}")))?;
    if let Err(message) = bridge_result {
        return Err(agent_event_delivery_error(message));
    }

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            send_channel_event(&on_event, crate::agent::StreamEvent::Error { error: msg })?;
            return Err(e);
        }
    };

    Ok(AssistantAnswer {
        answer: result.answer,
        references: result.references,
        used_ai: result.used_ai,
        model_name: model_config.map(|c| c.model.clone()),
        tool_labels: result.tool_labels,
        cards: Vec::new(),
    })
}

/// 用指定模型生成一段文本（单轮，非 agent 循环）。用于 starter prompt 动态生成等轻量场景。
#[tauri::command]
pub async fn generate_text_with_model(
    model_config: ModelConfig,
    system_prompt: String,
    prompt: String,
) -> Result<String, AppError> {
    generate_text_answer_with_model(&model_config, &system_prompt, &prompt).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// system prompt 必须包含真实固定的工具历史摘要格式（每个 locale 都要有）。
    /// 防回归：之前这声明曾误加在 executor.rs 的 DEFAULT_SYSTEM_PROMPT，但生产路径
    /// chat_work_assistant 始终传 Some(build_assistant_system_prompt(...))，unwrap_or
    /// 永远走不到，导致声明在生产里是死代码（codex 二轮 review 发现）。
    #[test]
    fn 各_locale的助手系统提示词必须引用真实固定机器格式并说明未知状态() {
        let cases = [
            (
                AppLocale::ZhCn,
                "`?` 表示旧数据状态未知，不能视为成功或失败",
            ),
            (
                AppLocale::ZhTw,
                "`?` 表示舊資料狀態未知，不能視為成功或失敗",
            ),
            (
                AppLocale::En,
                "`?` means legacy data has an unknown status and must not be treated as success or failure",
            ),
            (
                AppLocale::Ar,
                "تعني `?` أن حالة البيانات القديمة غير معروفة، ولا يجوز اعتبارها نجاحًا أو فشلًا",
            ),
        ];

        for (locale, unknown_hint) in cases {
            let prompt = build_assistant_system_prompt(locale);
            assert!(
                prompt.contains("[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]"),
                "locale {locale:?} 的 system prompt 未引用真实固定机器格式，got: {prompt}"
            );
            assert!(
                prompt.contains(unknown_hint),
                "locale {locale:?} 的 system prompt 未正确说明旧数据未知状态，got: {prompt}"
            );
        }
    }

    #[tokio::test]
    async fn 控制事件应等待桥接真实投递成功() {
        let (tx, rx) = crate::agent::StreamEventSender::channel(1);
        let send_task = async move {
            tx.send_control(crate::agent::StreamEvent::Done {
                answer: "完成".to_string(),
                references: vec![],
                tool_labels: vec![],
            })
            .await
        };
        let bridge = bridge_agent_events(rx, |event| {
            assert!(matches!(
                event,
                crate::agent::StreamEvent::Done { answer, .. } if answer == "完成"
            ));
            Ok(())
        });

        let (send_result, bridge_result) = tokio::join!(send_task, bridge);
        send_result.expect("实际投递成功后控制事件应返回成功");
        bridge_result.expect("桥接应正常结束");
    }

    #[tokio::test]
    async fn 桥接投递失败应反馈给控制事件发送方() {
        let (tx, rx) = crate::agent::StreamEventSender::channel(1);
        let send_task = async move {
            tx.send_control(crate::agent::StreamEvent::Done {
                answer: "无法送达".to_string(),
                references: vec![],
                tool_labels: vec![],
            })
            .await
        };
        let bridge = bridge_agent_events(rx, |_event| Err("Webview 已关闭".to_string()));

        let (send_result, bridge_result) = tokio::join!(send_task, bridge);
        assert_eq!(
            send_result.expect_err("外部投递失败必须反馈给控制事件发送方"),
            "Webview 已关闭"
        );
        assert_eq!(
            bridge_result.expect_err("桥接必须保留真实投递失败原因"),
            "Webview 已关闭"
        );
    }

    #[test]
    fn 空问题不应返回任何工具标签() {
        assert!(
            empty_question_tool_labels().is_empty(),
            "空问题没有执行工具，不应声明工具标签"
        );
    }

    fn sample_process_follow_up_history() -> Vec<AssistantChatMessage> {
        vec![
            AssistantChatMessage {
                role: "user".to_string(),
                content: "最近时间主要花在哪？".to_string(),
            },
            AssistantChatMessage {
                role: "assistant".to_string(),
                content: "## 结论\n\n- 这段时间更像是围绕少数主题持续推进。\n\n## 过程分析\n\n- 主要是编码开发相关 session。\n".to_string(),
            },
        ]
    }

    fn sample_stage_follow_up_history() -> Vec<AssistantChatMessage> {
        vec![
            AssistantChatMessage {
                role: "user".to_string(),
                content: "这周主要做了什么？".to_string(),
            },
            AssistantChatMessage {
                role: "assistant".to_string(),
                content: "## 结论\n\n- 这周主线是助手回答链路改造。\n".to_string(),
            },
        ]
    }

    #[test]
    fn 助手问题分类应识别阶段总结与过程复盘和证据追问() {
        assert_eq!(
            detect_assistant_question_kind("这周主要做了什么？", &[]),
            AssistantQuestionKind::StageSummary
        );
        assert_eq!(
            detect_assistant_question_kind("最近时间主要花在哪？", &[]),
            AssistantQuestionKind::ProcessRecap
        );
        assert_eq!(
            detect_assistant_question_kind("这个结论的依据是什么？", &[]),
            AssistantQuestionKind::EvidenceQuery
        );
    }

    #[test]
    fn 助手问题分类应继承上一轮过程复盘语境() {
        let history = sample_process_follow_up_history();

        assert_eq!(
            detect_assistant_question_kind("继续", &history),
            AssistantQuestionKind::ProcessRecap
        );
        assert_eq!(
            detect_assistant_question_kind("展开说说这个", &history),
            AssistantQuestionKind::ProcessRecap
        );
    }

    #[test]
    fn 助手问题分类应将依据追问优先识别为证据问题() {
        let history = sample_stage_follow_up_history();

        assert_eq!(
            detect_assistant_question_kind("那依据呢", &history),
            AssistantQuestionKind::EvidenceQuery
        );
        assert_eq!(
            detect_assistant_question_kind("这个结论怎么得出的", &history),
            AssistantQuestionKind::EvidenceQuery
        );
    }

    #[test]
    fn ai增强识别器应比基础模板更强承接助手上下文() {
        let history = vec![
            AssistantChatMessage {
                role: "user".to_string(),
                content: "这周主要做了什么？".to_string(),
            },
            AssistantChatMessage {
                role: "assistant".to_string(),
                content: "## 结论\n\n- 这周主线是助手回答链路改造。\n\n## 过程分析\n\n- 主要是编码开发相关 session。\n".to_string(),
            },
        ];

        assert_eq!(
            detect_assistant_question_kind_with_mode(
                "展开说说这个",
                &history,
                AssistantReasoningMode::Basic
            ),
            AssistantQuestionKind::StageSummary
        );
        assert_eq!(
            detect_assistant_question_kind_with_mode(
                "展开说说这个",
                &history,
                AssistantReasoningMode::AiEnhanced
            ),
            AssistantQuestionKind::ProcessRecap
        );
    }
}
