use crate::wechat::types::ContractError;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(crate) const CHUNK_SCHEMA_VERSION: &str = "chunk-v1";
pub(crate) const TOKEN_COUNTER_VERSION: &str = "v1";
pub(crate) const FTS_PRETOKEN_VERSION: &str = "fts-pretoken-v1";
const MAX_CHUNK_TOKENS: usize = 512;
const MAX_MESSAGES: usize = 40;
const MAX_GAP_MS: i64 = 30 * 60 * 1000;
const OVERLAP_MESSAGES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Self_,
    Other,
}

impl Direction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Self_ => "self",
            Self::Other => "other",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, ContractError> {
        match value {
            "self" => Ok(Self::Self_),
            "other" => Ok(Self::Other),
            _ => Err(ContractError::KbNotReady),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMessage {
    pub(crate) account_stable_id: String,
    pub(crate) conversation_stable_id: String,
    pub(crate) message_id: String,
    pub(crate) message_version_id: String,
    pub(crate) stable_message_key: String,
    pub(crate) created_at_ms: i64,
    pub(crate) source_ordinal: u64,
    pub(crate) sort_key: String,
    pub(crate) sender_key: String,
    pub(crate) direction: Direction,
    pub(crate) message_kind: String,
    pub(crate) content: String,
    pub(crate) content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChunkMember {
    pub(crate) message_id: String,
    pub(crate) message_version_id: String,
    pub(crate) message_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChunkDraft {
    pub(crate) account_stable_id: String,
    pub(crate) conversation_stable_id: String,
    pub(crate) chunk_index: u32,
    pub(crate) chunk_key: String,
    pub(crate) first_message_version_id: String,
    pub(crate) last_message_version_id: String,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    pub(crate) token_count: u32,
    pub(crate) content: String,
    pub(crate) content_hash: String,
    pub(crate) fts_terms: String,
    pub(crate) members: Vec<ChunkMember>,
}

#[derive(Clone, Debug)]
struct RenderedMessage {
    message: BuildMessage,
    text: String,
}

pub(crate) struct ChunkerState {
    import_snapshot_hash: String,
    fts_pretoken_version: String,
    cap: usize,
    account_stable_id: Option<String>,
    conversation_stable_id: Option<String>,
    current: Vec<RenderedMessage>,
    next_chunk_index: u32,
}

impl ChunkerState {
    pub(crate) fn new(
        import_snapshot_hash: &str,
        retrieval_token_budget: u32,
        fts_pretoken_version: &str,
    ) -> Result<Self, ContractError> {
        if !(256..=4096).contains(&retrieval_token_budget)
            || fts_pretoken_version != FTS_PRETOKEN_VERSION
        {
            return Err(ContractError::KbNotReady);
        }
        Ok(Self {
            import_snapshot_hash: import_snapshot_hash.into(),
            fts_pretoken_version: fts_pretoken_version.into(),
            cap: MAX_CHUNK_TOKENS.min(retrieval_token_budget as usize),
            account_stable_id: None,
            conversation_stable_id: None,
            current: Vec::new(),
            next_chunk_index: 0,
        })
    }

    pub(crate) fn push_page(
        &mut self,
        messages: &[BuildMessage],
    ) -> Result<Vec<ChunkDraft>, ContractError> {
        let mut drafts = Vec::new();
        for message in messages {
            self.validate_conversation(message)?;
            let rendered = RenderedMessage {
                message: message.clone(),
                text: render_message(message),
            };
            if token_count_v1(&rendered.text) > self.cap {
                return Err(ContractError::KbNotReady);
            }
            if self.current.is_empty() {
                self.current.push(rendered);
                continue;
            }
            let previous = self.current.last().ok_or(ContractError::KbNotReady)?;
            let time_boundary = message.created_at_ms - previous.message.created_at_ms > MAX_GAP_MS;
            let next_tokens =
                current_token_count(&self.current) + 1 + token_count_v1(&rendered.text);
            let size_boundary = self.current.len() == MAX_MESSAGES || next_tokens > self.cap;
            if !time_boundary && !size_boundary {
                self.current.push(rendered);
                continue;
            }

            let completed = std::mem::take(&mut self.current);
            drafts.push(self.build_current(&completed)?);
            if !time_boundary {
                let overlap_start = completed.len().saturating_sub(OVERLAP_MESSAGES);
                for overlap in &completed[overlap_start..] {
                    let candidate_tokens = if self.current.is_empty() {
                        token_count_v1(&overlap.text)
                    } else {
                        current_token_count(&self.current) + 1 + token_count_v1(&overlap.text)
                    };
                    let with_new = candidate_tokens + 1 + token_count_v1(&rendered.text);
                    if with_new <= self.cap && self.current.len() + 1 < MAX_MESSAGES {
                        self.current.push(overlap.clone());
                    }
                }
            }
            self.current.push(rendered);
        }
        Ok(drafts)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<ChunkDraft>, ContractError> {
        if self.current.is_empty() {
            return Ok(Vec::new());
        }
        let completed = std::mem::take(&mut self.current);
        Ok(vec![self.build_current(&completed)?])
    }

    fn validate_conversation(&mut self, message: &BuildMessage) -> Result<(), ContractError> {
        match (&self.account_stable_id, &self.conversation_stable_id) {
            (None, None) => {
                self.account_stable_id = Some(message.account_stable_id.clone());
                self.conversation_stable_id = Some(message.conversation_stable_id.clone());
                Ok(())
            }
            (Some(account), Some(conversation))
                if account == &message.account_stable_id
                    && conversation == &message.conversation_stable_id =>
            {
                Ok(())
            }
            _ => Err(ContractError::KbNotReady),
        }
    }

    fn build_current(&mut self, messages: &[RenderedMessage]) -> Result<ChunkDraft, ContractError> {
        let draft = build_draft(
            &self.import_snapshot_hash,
            self.next_chunk_index,
            &self.fts_pretoken_version,
            messages,
        )?;
        self.next_chunk_index = self
            .next_chunk_index
            .checked_add(1)
            .ok_or(ContractError::KbNotReady)?;
        Ok(draft)
    }
}

pub(crate) fn chunk_messages(
    import_snapshot_hash: &str,
    retrieval_token_budget: u32,
    messages: &[BuildMessage],
) -> Result<Vec<ChunkDraft>, ContractError> {
    let mut chunker = ChunkerState::new(
        import_snapshot_hash,
        retrieval_token_budget,
        FTS_PRETOKEN_VERSION,
    )?;
    let mut drafts = chunker.push_page(messages)?;
    drafts.extend(chunker.finish()?);
    Ok(drafts)
}

fn build_draft(
    import_snapshot_hash: &str,
    chunk_index: u32,
    fts_pretoken_version: &str,
    rendered: &[RenderedMessage],
) -> Result<ChunkDraft, ContractError> {
    let first = &rendered.first().ok_or(ContractError::KbNotReady)?.message;
    let last = &rendered.last().ok_or(ContractError::KbNotReady)?.message;
    let content = rendered
        .iter()
        .map(|message| message.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let chunk_key = chunk_key_v1(&[
        CHUNK_SCHEMA_VERSION,
        import_snapshot_hash,
        &first.account_stable_id,
        &first.conversation_stable_id,
        &chunk_index.to_string(),
        &first.stable_message_key,
        &last.stable_message_key,
    ]);
    Ok(ChunkDraft {
        account_stable_id: first.account_stable_id.clone(),
        conversation_stable_id: first.conversation_stable_id.clone(),
        chunk_index,
        chunk_key,
        first_message_version_id: first.message_version_id.clone(),
        last_message_version_id: last.message_version_id.clone(),
        started_at_ms: first.created_at_ms,
        ended_at_ms: last.created_at_ms,
        token_count: token_count_v1(&content) as u32,
        content_hash: chunk_content_hash_v1(&content),
        fts_terms: fts_pretoken(fts_pretoken_version, &content)?,
        content,
        members: rendered
            .iter()
            .enumerate()
            .map(|(message_index, message)| ChunkMember {
                message_id: message.message.message_id.clone(),
                message_version_id: message.message.message_version_id.clone(),
                message_index: message_index as u32,
            })
            .collect(),
    })
}

fn current_token_count(rendered: &[RenderedMessage]) -> usize {
    rendered
        .iter()
        .map(|message| token_count_v1(&message.text))
        .sum::<usize>()
        + rendered.len().saturating_sub(1)
}

pub(crate) fn draft_from_messages(
    import_snapshot_hash: &str,
    chunk_index: u32,
    fts_pretoken_version: &str,
    messages: &[BuildMessage],
) -> Result<ChunkDraft, ContractError> {
    let rendered = messages
        .iter()
        .cloned()
        .map(|message| RenderedMessage {
            text: render_message(&message),
            message,
        })
        .collect::<Vec<_>>();
    build_draft(
        import_snapshot_hash,
        chunk_index,
        fts_pretoken_version,
        &rendered,
    )
}

pub(crate) fn token_count_v1(text: &str) -> usize {
    text.len()
}

pub(crate) fn fts_pretoken_v1(text: &str) -> String {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        if characters[index].is_ascii_alphanumeric() {
            let start = index;
            while index < characters.len() && characters[index].is_ascii_alphanumeric() {
                index += 1;
            }
            push_term(
                characters[start..index]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
                &mut terms,
                &mut seen,
            );
        } else if is_cjk(characters[index]) {
            let start = index;
            while index < characters.len() && is_cjk(characters[index]) {
                index += 1;
            }
            let run = &characters[start..index];
            for width in [2, 3] {
                for window in run.windows(width) {
                    push_term(window.iter().collect(), &mut terms, &mut seen);
                }
            }
        } else {
            index += 1;
        }
    }
    terms.join(" ")
}

pub(crate) fn fts_pretoken(version: &str, text: &str) -> Result<String, ContractError> {
    match version {
        FTS_PRETOKEN_VERSION => Ok(fts_pretoken_v1(text)),
        _ => Err(ContractError::KbNotReady),
    }
}

pub(crate) fn fts_match_query_v1(query: &str) -> Option<String> {
    let terms = fts_pretoken_v1(query);
    (!terms.is_empty()).then(|| {
        terms
            .split_whitespace()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    })
}

pub(crate) fn fts_match_query(version: &str, query: &str) -> Result<Option<String>, ContractError> {
    match version {
        FTS_PRETOKEN_VERSION => Ok(fts_match_query_v1(query)),
        _ => Err(ContractError::KbNotReady),
    }
}

fn push_term(term: String, terms: &mut Vec<String>, seen: &mut HashSet<String>) {
    if !term.is_empty() && seen.insert(term.clone()) {
        terms.push(term);
    }
}

fn is_cjk(character: char) -> bool {
    matches!(character, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

fn render_message(message: &BuildMessage) -> String {
    format!(
        "[{}][{}][{}] {}",
        message.created_at_ms,
        message.direction.as_str(),
        message.sender_key,
        message.content
    )
}

fn versioned_hash(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hex::encode(hash.finalize())
}

pub(crate) fn chunk_content_hash_v1(content: &str) -> String {
    versioned_hash(&[CHUNK_SCHEMA_VERSION, content])
}

fn chunk_key_v1(parts: &[&str]) -> String {
    versioned_hash(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(index: usize, created_at_ms: i64, content: &str) -> BuildMessage {
        BuildMessage {
            account_stable_id: "account-fixture".into(),
            conversation_stable_id: "conversation-fixture".into(),
            message_id: format!("message-{index}"),
            message_version_id: format!("version-{index}"),
            stable_message_key: format!("stable-{index:03}"),
            created_at_ms,
            source_ordinal: index as u64,
            sort_key: format!("sort-{index:03}"),
            sender_key: "account-fixture".into(),
            direction: Direction::Self_,
            message_kind: "text".into(),
            content: content.into(),
            content_hash: versioned_hash(&[content]),
        }
    }

    #[test]
    fn pretoken_covers_chinese_ascii_and_ignores_query_syntax() {
        let terms = fts_pretoken_v1("ProjectA 知识库迁移");
        assert!(terms.contains("projecta"));
        assert!(terms.contains("迁移"));
        assert!(terms.contains("知识库"));
        assert_eq!(
            fts_match_query_v1("x OR y").unwrap(),
            "\"x\" AND \"or\" AND \"y\""
        );
    }

    #[test]
    fn chunk_boundaries_are_deterministic_and_time_gaps_do_not_overlap() {
        let large = "甲".repeat(100);
        let messages = vec![
            message(0, 0, &large),
            message(1, 1, &large),
            message(2, MAX_GAP_MS + 2, "结束"),
        ];
        let first = chunk_messages("snapshot", 512, &messages).unwrap();
        let second = chunk_messages("snapshot", 512, &messages).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        assert_eq!(first[1].members.len(), 1);
        assert_eq!(first[2].members.len(), 1);
        assert!(first.iter().all(|chunk| chunk.token_count <= 512));
    }

    fn stream_with_page_size(messages: &[BuildMessage], page_size: usize) -> Vec<ChunkDraft> {
        let mut chunker = ChunkerState::new("snapshot", 512, FTS_PRETOKEN_VERSION).unwrap();
        let mut drafts = Vec::new();
        for page in messages.chunks(page_size) {
            drafts.extend(chunker.push_page(page).unwrap());
        }
        drafts.extend(chunker.finish().unwrap());
        drafts
    }

    #[test]
    fn streaming_chunker_is_page_size_independent_across_all_boundaries() {
        let mut messages = (0..260)
            .map(|index| {
                let content = if index == 90 || index == 91 {
                    "甲".repeat(100)
                } else {
                    format!("message-{index}")
                };
                let created_at_ms = if index < 180 {
                    index as i64
                } else {
                    MAX_GAP_MS + index as i64 + 1
                };
                message(index, created_at_ms, &content)
            })
            .collect::<Vec<_>>();
        messages[179].created_at_ms = 179;

        let single_page = stream_with_page_size(&messages, messages.len());
        for page_size in [1, 17, 64, 128, 256] {
            assert_eq!(stream_with_page_size(&messages, page_size), single_page);
        }
        assert_eq!(
            single_page
                .iter()
                .map(|draft| draft.chunk_index)
                .collect::<Vec<_>>(),
            (0..single_page.len() as u32).collect::<Vec<_>>()
        );
        assert!(single_page.iter().all(|draft| draft.members.len() <= 40));
        for adjacent in single_page.windows(2) {
            let previous = adjacent[0]
                .members
                .iter()
                .map(|member| member.message_version_id.as_str())
                .collect::<HashSet<_>>();
            let overlap = adjacent[1]
                .members
                .iter()
                .filter(|member| previous.contains(member.message_version_id.as_str()))
                .count();
            assert!(overlap <= OVERLAP_MESSAGES);
        }
        let before_gap = single_page
            .iter()
            .position(|draft| draft.last_message_version_id == "version-179")
            .unwrap();
        assert_eq!(
            single_page[before_gap + 1].first_message_version_id,
            "version-180"
        );
    }

    #[test]
    fn oversized_message_fails_without_truncation() {
        assert_eq!(
            chunk_messages("snapshot", 512, &[message(0, 0, &"甲".repeat(600))]),
            Err(ContractError::KbNotReady)
        );
    }
}
