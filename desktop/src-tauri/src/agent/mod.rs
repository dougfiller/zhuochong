//! Agent 模块 — 第三代 Agentic 架构
//!
//! 五层结构：Tools → Model → Executor → Orchestrator
//! 当前进度：Stage 1-5 全部完成 ✅

pub mod events;
pub mod executor;
pub mod model;
pub mod orchestrator;
pub mod tools;

pub use events::{StreamEvent, StreamEventSender};
pub use model::Message;
pub use orchestrator::Orchestrator;
pub use tools::{
    ActionBridge, AssistantAction, AssistantRuntime, ConfirmBridge, ConfirmDecision, WebToolsConfig,
};
