mod fixtures;
mod model_contract;
mod ocr;
pub(crate) mod capture;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod profiles;
pub(crate) mod runtime;
pub(crate) mod state_machine;
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
    use super::model_contract::generate_rag_reply;

    const _: () = {
        let _ = generate_rag_reply;
    };
}

#[cfg(feature = "wechat-contract-probe-m2-m1")]
mod m2_m1_probe {
    use super::model_contract::generate_m1_reply;

    const _: () = {
        let _ = generate_m1_reply;
    };
}
