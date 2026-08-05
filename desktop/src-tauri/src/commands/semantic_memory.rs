//! 语义记忆（屏幕级数字记忆）。
//!
//! 把已采集的 OCR 文本 / 窗口标题 / URL 聚合成"记忆块"，经嵌入模型向量化后
//! 支持语义检索——"上周那篇讲 Rust async 的文章在哪看的"直接翻出当时的
//! URL + OCR 摘要。隐私默认关：本地 Ollama 时数据不出机；OpenAI 兼容云端
//! 接口需用户在设置里显式选择。

use crate::database::{
    encode_embedding, normalize_embedding, Activity, SemanticMemoryHit, SemanticMemoryStats,
};
use crate::error::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::State;

use super::shared::{collect_privacy_filters, filter_activities_by_privacy};

/// 单轮索引最多消费的活动行数（前端循环调用直至 done）。
const INDEX_ACTIVITY_BATCH: usize = 1500;
/// 单轮索引最多嵌入的块数（控制单次调用时长）。
const EMBED_BATCH: usize = 32;
/// 记忆块正文上限（字符）。
const CHUNK_CONTENT_MAX_CHARS: usize = 1200;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SemanticIndexProgress {
    /// 本轮消费的活动行数
    pub processed_activities: usize,
    /// 本轮新写入/更新的记忆块数
    pub upserted_chunks: usize,
    /// 本轮完成嵌入的块数
    pub embedded_chunks: usize,
    /// 仍待嵌入的块数
    pub pending_embeddings: usize,
    /// 活动游标是否已追平（true 且 pending 为 0 表示索引完成）
    pub activities_done: bool,
    pub stats: SemanticMemoryStats,
}

#[derive(Clone)]
struct EmbeddingConfig {
    provider: String,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

fn load_embedding_config(state: &Arc<Mutex<AppState>>) -> Result<EmbeddingConfig, AppError> {
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    if !s.config.memory_semantic_enabled {
        return Err(AppError::Config(
            "语义记忆未启用，请到「设置 → AI 模型」开启".to_string(),
        ));
    }
    Ok(EmbeddingConfig {
        provider: s.config.embedding_provider.clone(),
        endpoint: s.config.embedding_endpoint.trim().trim_end_matches('/').to_string(),
        model: s.config.embedding_model.clone(),
        api_key: s.config.embedding_api_key.clone(),
    })
}

fn embedding_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Unknown(format!("创建嵌入 HTTP 客户端失败: {e}")))
}

/// 调用嵌入服务，返回与输入等长的归一化向量列表。
async fn embed_texts(
    config: &EmbeddingConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let client = embedding_client()?;

    if config.provider == "openai" {
        // OpenAI 兼容 /v1/embeddings：批量一次请求
        let url = if config.endpoint.contains("/v1") {
            format!("{}/embeddings", config.endpoint)
        } else {
            format!("{}/v1/embeddings", config.endpoint)
        };
        let mut request = client
            .post(&url)
            .json(&serde_json::json!({ "model": config.model, "input": texts }));
        if let Some(key) = config.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let response = request.send().await.map_err(|e| {
            AppError::Analysis(format!("嵌入请求失败: {e}"))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let body: String = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            return Err(AppError::Analysis(format!(
                "嵌入服务返回 HTTP {status}: {body}"
            )));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::Analysis(format!("嵌入响应解析失败: {e}")))?;
        let data = payload["data"]
            .as_array()
            .ok_or_else(|| AppError::Analysis("嵌入响应缺少 data 字段".to_string()))?;
        let mut vectors = Vec::with_capacity(texts.len());
        for item in data {
            let vector: Vec<f32> = item["embedding"]
                .as_array()
                .ok_or_else(|| AppError::Analysis("嵌入响应缺少 embedding 字段".to_string()))?
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            vectors.push(normalize_embedding(vector));
        }
        if vectors.len() != texts.len() {
            return Err(AppError::Analysis(format!(
                "嵌入结果数量不匹配: 期望 {} 实际 {}",
                texts.len(),
                vectors.len()
            )));
        }
        Ok(vectors)
    } else {
        // Ollama /api/embeddings：逐条请求（本地服务，延迟低）
        let url = format!("{}/api/embeddings", config.endpoint);
        let mut vectors = Vec::with_capacity(texts.len());
        for text in texts {
            let response = client
                .post(&url)
                .json(&serde_json::json!({ "model": config.model, "prompt": text }))
                .send()
                .await
                .map_err(|e| {
                    AppError::Analysis(format!(
                        "Ollama 嵌入请求失败（请确认 Ollama 已启动并已拉取模型 {}）: {e}",
                        config.model
                    ))
                })?;
            if !response.status().is_success() {
                let status = response.status();
                let body: String = response
                    .text()
                    .await
                    .unwrap_or_default()
                    .chars()
                    .take(300)
                    .collect();
                return Err(AppError::Analysis(format!(
                    "Ollama 嵌入返回 HTTP {status}: {body}"
                )));
            }
            let payload: serde_json::Value = response
                .json()
                .await
                .map_err(|e| AppError::Analysis(format!("Ollama 嵌入响应解析失败: {e}")))?;
            let vector: Vec<f32> = payload["embedding"]
                .as_array()
                .ok_or_else(|| AppError::Analysis("Ollama 嵌入响应缺少 embedding 字段".to_string()))?
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            vectors.push(normalize_embedding(vector));
        }
        Ok(vectors)
    }
}

/// 记忆块草稿（分块结果，纯数据）。
#[derive(Debug, Clone)]
pub(crate) struct ChunkDraft {
    pub chunk_key: String,
    pub date: String,
    pub app_name: String,
    pub title: String,
    pub browser_url: Option<String>,
    pub content: String,
    pub last_activity_id: i64,
}

fn local_date_of_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn domain_of(url: &str) -> String {
    let trimmed = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_string()
}

/// 把活动记录聚合成记忆块草稿（纯函数，可单测）：
/// 分组键 = (本地日期, 应用, 域名或标题前 80 字)；正文 = 标题 + URL + 组内最长 OCR 摘要。
/// 跳过脱敏占位记录与无内容记录。
pub(crate) fn build_chunk_drafts(activities: &[Activity]) -> Vec<ChunkDraft> {
    struct Group {
        date: String,
        app_name: String,
        title: String,
        browser_url: Option<String>,
        best_ocr: String,
        titles: Vec<String>,
        last_activity_id: i64,
    }

    let mut groups: HashMap<String, Group> = HashMap::new();
    for activity in activities {
        let Some(id) = activity.id else { continue };
        let title = activity.window_title.trim();
        if title.is_empty() || title == "[内容已脱敏]" {
            continue;
        }
        let ocr = activity.ocr_text.as_deref().unwrap_or("").trim();
        // 标题过短且无 OCR、无 URL 的记录没有可检索价值
        if ocr.is_empty() && activity.browser_url.is_none() && title.chars().count() < 4 {
            continue;
        }

        let date = local_date_of_timestamp(activity.timestamp);
        let group_hint = activity
            .browser_url
            .as_deref()
            .map(domain_of)
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| title.chars().take(80).collect());
        let chunk_key = format!("{date}|{}|{group_hint}", activity.app_name);

        let entry = groups.entry(chunk_key).or_insert_with(|| Group {
            date,
            app_name: activity.app_name.clone(),
            title: title.to_string(),
            browser_url: activity.browser_url.clone(),
            best_ocr: String::new(),
            titles: Vec::new(),
            last_activity_id: id,
        });
        entry.last_activity_id = entry.last_activity_id.max(id);
        if ocr.chars().count() > entry.best_ocr.chars().count() {
            entry.best_ocr = ocr.to_string();
        }
        if entry.browser_url.is_none() {
            entry.browser_url = activity.browser_url.clone();
        }
        let title_owned = title.to_string();
        if !entry.titles.contains(&title_owned) && entry.titles.len() < 5 {
            entry.titles.push(title_owned);
        }
    }

    let mut drafts: Vec<ChunkDraft> = groups
        .into_iter()
        .map(|(chunk_key, group)| {
            let mut content = format!("{} | {}", group.app_name, group.titles.join(" / "));
            if let Some(url) = &group.browser_url {
                content.push_str(&format!("\n{url}"));
            }
            if !group.best_ocr.is_empty() {
                content.push('\n');
                content.push_str(&group.best_ocr);
            }
            let content: String = content.chars().take(CHUNK_CONTENT_MAX_CHARS).collect();
            ChunkDraft {
                chunk_key,
                date: group.date,
                app_name: group.app_name,
                title: group.title,
                browser_url: group.browser_url,
                content,
                last_activity_id: group.last_activity_id,
            }
        })
        .collect();
    drafts.sort_by_key(|d| d.last_activity_id);
    drafts
}

/// 增量索引一轮：消费活动游标之后的记录 → 分块入库 → 嵌入一批待处理块。
/// 前端循环调用直到 activities_done && pending_embeddings == 0。
#[tauri::command]
pub async fn index_semantic_memory(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<SemanticIndexProgress, AppError> {
    let state_arc: Arc<Mutex<AppState>> = state.inner().clone();
    let embedding = load_embedding_config(&state_arc)?;

    // ① 消费活动游标之后的记录，聚合成记忆块
    let (database, drafts, processed, done) = {
        let s = state_arc.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        let cursor = s.database.semantic_memory_stats()?.last_indexed_activity_id;
        let activities = s.database.get_activities_after_id(cursor, INDEX_ACTIVITY_BATCH)?;
        let processed = activities.len();
        let done = processed < INDEX_ACTIVITY_BATCH;
        // 游标推进基准：本批最大活动 id（在隐私过滤前取，避免整批被过滤时死循环）
        let max_id = activities.iter().filter_map(|a| a.id).max().unwrap_or(cursor);
        // 隐私过滤：被忽略应用/排除域名的记录不进入语义索引
        let (ignored_apps, excluded_domains) = collect_privacy_filters(&s);
        let activities = filter_activities_by_privacy(activities, &ignored_apps, &excluded_domains);
        let mut drafts = build_chunk_drafts(&activities);
        if let Some(last) = drafts.last_mut() {
            // 把游标推进到本批最大 id（含被过滤记录），下一轮从新位置继续
            last.last_activity_id = last.last_activity_id.max(max_id);
        } else if max_id > cursor {
            // 本批全部被过滤/无价值：写入占位游标块推进进度（检索时会跳过）
            drafts.push(ChunkDraft {
                chunk_key: "__cursor__".to_string(),
                date: "1970-01-01".to_string(),
                app_name: "__cursor__".to_string(),
                title: "__cursor__".to_string(),
                browser_url: None,
                content: "__cursor__".to_string(),
                last_activity_id: max_id,
            });
        }
        (s.database.clone(), drafts, processed, done)
    };

    let mut upserted = 0usize;
    for draft in &drafts {
        database.upsert_memory_chunk(
            &draft.chunk_key,
            &draft.date,
            &draft.app_name,
            &draft.title,
            draft.browser_url.as_deref(),
            &draft.content,
            draft.last_activity_id,
        )?;
        upserted += 1;
    }

    // ② 嵌入一批待处理块（游标块不参与检索但也会被嵌入一次，代价可忽略）
    let pending = database.get_unembedded_memory_chunks(EMBED_BATCH)?;
    let mut embedded = 0usize;
    if !pending.is_empty() {
        let texts: Vec<String> = pending.iter().map(|(_, c)| c.clone()).collect();
        let vectors = embed_texts(&embedding, &texts).await?;
        for ((chunk_id, _), vector) in pending.iter().zip(vectors.iter()) {
            database.set_memory_chunk_embedding(*chunk_id, &encode_embedding(vector))?;
            embedded += 1;
        }
    }

    let stats = database.semantic_memory_stats()?;
    let pending_embeddings = stats.total_chunks.saturating_sub(stats.embedded_chunks);
    Ok(SemanticIndexProgress {
        processed_activities: processed,
        upserted_chunks: upserted,
        embedded_chunks: embedded,
        pending_embeddings,
        activities_done: done,
        stats,
    })
}

/// 测试嵌入模型连通性：发一条探针文本,返回向量维度与耗时。
/// 接收设置页表单的当前值直接测试（而非后端已保存配置）：设置更改需点「保存」
/// 才落盘，此前命令读已保存配置，填完表单未保存就点「测试模型」会测到旧配置。
/// 所测即所见；实际索引仍以保存后的配置为准。不要求先启用总开关（用户通常想先测通再开启）。
#[tauri::command]
pub async fn test_embedding_model(
    provider: String,
    endpoint: String,
    model: String,
    api_key: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();
    if endpoint.is_empty() || model.is_empty() {
        return Err(AppError::Config("请先填写嵌入服务地址与模型名".to_string()));
    }
    let embedding = EmbeddingConfig {
        provider,
        endpoint,
        model,
        api_key,
    };

    let started = std::time::Instant::now();
    let vectors = embed_texts(&embedding, &["connection test".to_string()]).await?;
    let dimension = vectors.first().map(|v| v.len()).unwrap_or(0);
    if dimension == 0 {
        return Err(AppError::Analysis("嵌入服务返回了空向量".to_string()));
    }
    Ok(serde_json::json!({
        "dimension": dimension,
        "latencyMs": started.elapsed().as_millis() as u64,
    }))
}

/// 索引状态（设置页/记忆页展示用）。
#[tauri::command]
pub async fn semantic_memory_status(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<SemanticMemoryStats, AppError> {
    let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.database.semantic_memory_stats()
}



/// 内部实现：也供助手 semantic_search 工具桥接调用。
pub(crate) async fn search_semantic_memory_inner(
    state_arc: &Arc<Mutex<AppState>>,
    query: &str,
    limit: usize,
) -> Result<Vec<SemanticMemoryHit>, AppError> {
    let embedding = load_embedding_config(state_arc)?;
    let database = {
        let s = state_arc.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        s.database.clone()
    };

    // ① 向量召回。嵌入服务不可用（如 Ollama 未启动）时不报错，
    // 降级为纯关键词检索——记忆能力保持"基础模板级"可用。
    let semantic_hits = match embed_texts(&embedding, &[query.to_string()]).await {
        Ok(vectors) => match vectors.into_iter().next() {
            Some(query_vector) => {
                database.search_memory_chunks_semantic(&query_vector, limit.max(20))?
            }
            None => Vec::new(),
        },
        Err(e) => {
            log::warn!("嵌入服务不可用，语义检索降级为关键词检索: {e}");
            Vec::new()
        }
    };

    // ② 关键词召回（复用既有 FTS 检索），RRF 融合：score = Σ 1/(60+rank)
    let fts_hits = database
        .search_memory(query, None, None, limit.max(20))
        .unwrap_or_default();

    const RRF_K: f64 = 60.0;
    let mut fused: HashMap<String, SemanticMemoryHit> = HashMap::new();
    for (rank, hit) in semantic_hits.into_iter().enumerate() {
        if hit.app_name == "__cursor__" {
            continue;
        }
        let key = format!("{}|{}|{}", hit.date, hit.app_name, hit.title);
        let entry = fused.entry(key).or_insert_with(|| SemanticMemoryHit {
            score: 0.0,
            ..hit
        });
        entry.score += 1.0 / (RRF_K + rank as f64 + 1.0);
    }
    for (rank, item) in fts_hits.into_iter().enumerate() {
        let app_name = item.app_name.clone().unwrap_or_default();
        let key = format!("{}|{}|{}", item.date, app_name, item.title);
        let entry = fused.entry(key).or_insert_with(|| SemanticMemoryHit {
            chunk_id: 0,
            date: item.date.clone(),
            app_name,
            title: item.title.clone(),
            browser_url: item.browser_url.clone(),
            excerpt: item.excerpt.clone(),
            score: 0.0,
        });
        entry.score += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    let mut results: Vec<SemanticMemoryHit> = fused.into_values().collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(id: i64, ts: i64, app: &str, title: &str, url: Option<&str>, ocr: Option<&str>) -> Activity {
        Activity {
            id: Some(id),
            timestamp: ts,
            app_name: app.to_string(),
            window_title: title.to_string(),
            screenshot_path: String::new(),
            ocr_text: ocr.map(ToString::to_string),
            category: "other".to_string(),
            duration: 30,
            browser_url: url.map(ToString::to_string),
            executable_path: None,
            semantic_category: None,
            semantic_confidence: None,
            screenshot_url: None,
        }
    }

    #[test]
    fn 分块应按日期应用域名聚合并保留最长ocr() {
        let ts = 1_753_500_000; // 同一天
        let acts = vec![
            activity(1, ts, "Chrome", "Rust async 详解 - 博客", Some("https://blog.example.com/rust-async"), Some("短 OCR")),
            activity(2, ts + 60, "Chrome", "Rust async 详解 - 博客", Some("https://blog.example.com/rust-async?p=2"), Some("这是一段更长的 OCR 文本内容示例")),
            activity(3, ts + 120, "Xcode", "main.rs — work-review", None, Some("fn main() {}")),
        ];
        let drafts = build_chunk_drafts(&acts);
        assert_eq!(drafts.len(), 2, "同域名网页应合并，Xcode 单独一块");
        let chrome = drafts.iter().find(|d| d.app_name == "Chrome").unwrap();
        assert!(chrome.content.contains("更长的 OCR"), "应保留组内最长 OCR");
        assert!(chrome.content.contains("blog.example.com"));
        assert_eq!(chrome.last_activity_id, 2);
    }

    #[test]
    fn 分块应跳过脱敏与无价值记录() {
        let ts = 1_753_500_000;
        let acts = vec![
            activity(1, ts, "App", "[内容已脱敏]", None, Some("不应出现")),
            activity(2, ts, "App", "ab", None, None),
        ];
        assert!(build_chunk_drafts(&acts).is_empty());
    }

    #[test]
    fn 域名提取应容忍协议与路径() {
        assert_eq!(domain_of("https://a.b.com/x?y=1"), "a.b.com");
        assert_eq!(domain_of("http://a.b.com"), "a.b.com");
        assert_eq!(domain_of("a.b.com/path"), "a.b.com");
    }
}
