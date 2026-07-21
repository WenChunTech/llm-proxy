use llm_proxy::protocol::SUPPORTED_PROTOCOL_CONVERSIONS;
use llm_proxy::provider::types::ProviderType;

#[test]
fn protocol_conversion_matrix_covers_all_non_identity_core_pairs() {
    let protocols = [
        ProviderType::Chat,
        ProviderType::Responses,
        ProviderType::Claude,
        ProviderType::Gemini,
    ];

    for source in protocols {
        for target in protocols {
            if source == target {
                continue;
            }
            assert!(
                SUPPORTED_PROTOCOL_CONVERSIONS.contains(&(source, target)),
                "missing conversion matrix entry: {source:?} -> {target:?}"
            );
        }
    }
}

#[test]
fn protocol_conversion_matrix_excludes_provider_aliases() {
    assert!(
        !SUPPORTED_PROTOCOL_CONVERSIONS
            .iter()
            .any(
                |(source, target)| matches!(source, ProviderType::Codex | ProviderType::Grok)
                    || matches!(target, ProviderType::Codex | ProviderType::Grok)
            )
    );
}

