use crate::wechat::types::ContractError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum KnowledgeScope {
    #[serde(rename = "conversation")]
    Conversation { id: String },
    #[serde(rename = "selected_conversations")]
    SelectedConversations { ids: Vec<String> },
    #[serde(rename = "global_user_selected")]
    GlobalUserSelected,
}

impl KnowledgeScope {
    pub(crate) fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Conversation { id } if invalid_id(id) => Err(ContractError::KbScopeUnresolved),
            Self::SelectedConversations { ids }
                if ids.is_empty()
                    || ids.len() > 32
                    || ids.iter().any(|id| invalid_id(id))
                    || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() =>
            {
                Err(ContractError::KbScopeUnresolved)
            }
            _ => Ok(()),
        }
    }
}

fn invalid_id(id: &str) -> bool {
    id.trim().is_empty() || id.contains('\0')
}

#[allow(unused_imports)]
pub(crate) use super::retrieve::{
    KnowledgeError, KnowledgeRetrieveRequest, RetrievalMode, RetrievalStatus,
    RetrievedContextDirection, RetrievedContextHit, RetrievedContextLine, RetrievedContextParts,
    RetrievedReply,
};

#[cfg(test)]
pub(crate) use super::retrieve::{retrieval_fixture, RetrievalOutcome};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_wire_shape_is_explicit_and_invalid_selections_are_rejected() {
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::Conversation {
                id: "conversation-stable-id".into()
            })
            .unwrap(),
            "{\"kind\":\"conversation\",\"id\":\"conversation-stable-id\"}"
        );
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::SelectedConversations {
                ids: vec!["conversation-a".into(), "conversation-b".into()]
            })
            .unwrap(),
            "{\"kind\":\"selected_conversations\",\"ids\":[\"conversation-a\",\"conversation-b\"]}"
        );
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::GlobalUserSelected).unwrap(),
            "{\"kind\":\"global_user_selected\"}"
        );
        for invalid in [
            KnowledgeScope::SelectedConversations { ids: vec![] },
            KnowledgeScope::SelectedConversations {
                ids: vec!["duplicate".into(), "duplicate".into()],
            },
        ] {
            assert_eq!(invalid.validate(), Err(ContractError::KbScopeUnresolved));
        }
    }
}
