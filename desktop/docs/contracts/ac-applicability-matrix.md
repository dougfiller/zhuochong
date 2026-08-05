# 微信与知识库验收适用矩阵（步骤 4）

本矩阵冻结契约责任，不把尚未实现的运行时能力标记为通过。M1 仅支持单聊实时回复；群聊仅能作为用户明确选择的知识范围。

| ac_id | phase(M1\|M2\|conditional) | applicability | proof_type | owner_step | blocking | evidence_target |
| --- | --- | --- | --- | --- | --- | --- |
| AC-WX-01 | M1 | required | unit | step-4 | yes | `wechat::fixtures::tests::ocr_fixtures_preserve_the_input_boundary` |
| AC-WX-02 | M1 | deferred | windows-manual | step-5 | yes | Windows capture/OCR evidence |
| AC-WX-03 | M1 | deferred | windows-manual | step-6 | yes | foreground/profile evidence |
| AC-WX-04 | M1 | deferred | integration | step-6 | yes | user copy/dismiss flow |
| AC-WX-05 | M1 | required | contract | step-4 | yes | `empty_ocr.json`, stable WX error codes |
| AC-WX-06 | M1 | required | unit | step-4 | yes | `wechat::state_machine::tests` |
| AC-PET-01 | conditional | deferred | windows-manual | step-7 | no | pet display proof |
| AC-PET-02 | conditional | deferred | integration | step-7 | no | no unintended chat processing |
| AC-KB-01 | M2 | deferred | integration | step-8 | yes | independent `knowledge.sqlite` lifecycle |
| AC-KB-02 | M2 | required | contract | step-4 | yes | three `KnowledgeScope` JSON shapes |
| AC-KB-03 | M2 | required | contract | step-4 | yes | empty selected scope rejects `KB_SCOPE_UNRESOLVED` |
| AC-KB-04 | M2 | deferred | integration | step-8 | yes | import/dedup fixture consumption |
| AC-KB-05 | M2 | conditional-not-enabled | contract | step-4 | yes | unsupported/ambiguous fixture contracts |
| AC-RAG-01 | M2 | required | unit | step-4 | yes | `wechat::model_contract::tests::no_hit_is_the_only_empty_retrieval_that_reaches_generation` |
| AC-RAG-02 | M2 | deferred | integration | step-9 | yes | real retrieve-before-model trace |
| AC-RAG-03 | M2 | required | contract | step-4 | yes | `wechat::model_contract::tests::retrieval_failure_ends_m2_before_context_or_model_transport` |
| AC-RAG-04 | M2 | required | compile | step-4 | yes | `wechat-contract-probe-private-constructors` expected-fail check |
| AC-RAG-05 | conditional | required | compile | step-4 | yes | `wechat-contract-probe-m1-rag` and `wechat-contract-probe-m2-m1` expected-fail checks |
| AC-CONTRACT-01 | conditional | required | unit | step-4 | yes | strict `stage_seq` and legal transitions |
| AC-CONTRACT-02 | conditional | required | contract | step-4 | yes | capture/suggestion/binding/observation distinct newtypes |
| AC-CONTRACT-03 | conditional | required | contract | step-4 | yes | all stable `WX_*`, `KB_*`, `LLM_FAILED` codes |
| AC-PRIVACY-01 | conditional | required | contract | step-4 | yes | hand-written fixture review; no chat export/source path/hash |

`no_hit` is a successful retrieval outcome and may continue to M2 generation. `KB_NOT_READY`、`KB_SCOPE_UNRESOLVED` and `KB_RETRIEVAL_FAILED` terminate the request and may not create a model context. M1 only accepts single-chat capture metadata; group chats remain a user-selected knowledge scope only. Feature checks are intentionally isolated from the default Work Review build: `wechat-contract-check` turns “no M1/M2 feature” into the expected compile failure while the normal product target remains feature-free.
