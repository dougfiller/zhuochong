mod model_contract;
mod fixtures;
pub(crate) mod state_machine;
pub(crate) mod types;

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
