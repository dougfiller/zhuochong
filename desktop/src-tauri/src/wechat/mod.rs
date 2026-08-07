pub(crate) mod binding;
pub(crate) mod capture;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod content;
mod fixtures;
mod model_client;
mod model_contract;
mod ocr;
pub(crate) mod profiles;
#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
mod reply_flow;
pub(crate) mod runtime;
pub(crate) mod state_machine;
pub(crate) mod trace;
pub(crate) mod types;
pub(crate) mod window_identity;

pub(crate) use runtime::{CaptureCoordinator, WechatReplyRuntime};

#[cfg(feature = "wechat-contract-probe-private-constructors")]
mod untrusted_constructor_probe {
    use super::model_contract::ModelKnowledgeContext;
    use crate::knowledge::types::RetrievedReply;

    fn cannot_construct_private_contract_types() {
        let _ = RetrievedReply {};
        let _ = ModelKnowledgeContext {};
    }
}

#[cfg(feature = "wechat-contract-probe-m1-rag")]
mod m1_rag_probe {
    use super::model_client::WechatReplyModelClient;

    const _: () = {
        let _ = WechatReplyModelClient::generate_m2;
    };
}

#[cfg(feature = "wechat-contract-probe-m2-m1")]
mod m2_m1_probe {
    use super::model_client::WechatReplyModelClient;

    const _: () = {
        let _ = WechatReplyModelClient::generate_m1;
        let _ = super::reply_flow::generate_m1_wechat_reply;
    };
}
