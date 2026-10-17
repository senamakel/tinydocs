//! Focused classification and digest tests for the TinyDocs client.

use super::*;

fn failure(kind: ModuleCallError, wire_name: &str, message: &str) -> ModuleCallFailure {
    ModuleCallFailure {
        kind,
        wire_name: wire_name.to_string(),
        message: message.to_string(),
    }
}

#[test]
fn only_the_tinydocs_invalid_input_wire_name_is_model_correctable() {
    let invalid = classify_failure(failure(
        ModuleCallError::ModuleFault,
        INVALID_INPUT_WIRE_NAME,
        "invalid section heading",
    ));
    assert_eq!(
        invalid,
        DocumentCallError::InvalidInput("invalid section heading".to_string())
    );

    for wire_name in [
        "ai.tinyhumans.tinydocs.Error.GenerationFailed",
        "ai.tinyhumans.tinydocs.Error.ExtractionFailed",
        "ai.tinyhumans.tinydocs.Error.TransferFailed",
        "ai.tinyhumans.tinydocs.Error.UnknownOutput",
        "ai.tinyhumans.tinydocs.Error.SomethingNew",
    ] {
        assert!(matches!(
            classify_failure(failure(
                ModuleCallError::ModuleFault,
                wire_name,
                "untrusted remote text",
            )),
            DocumentCallError::Failed(_)
        ));
    }
}

#[test]
fn unavailable_sdk_outcomes_stay_distinct_from_module_failures() {
    assert!(matches!(
        classify_failure(failure(
            ModuleCallError::Unavailable,
            "ai.tinyhumans.tinybus.Error.ModuleUnavailable",
            "module is unavailable",
        )),
        DocumentCallError::Unavailable(_)
    ));
    assert!(matches!(
        classify_sdk_error(ModuleCallError::Unavailable),
        DocumentCallError::Unavailable(_)
    ));
    assert!(matches!(
        classify_sdk_error(ModuleCallError::IncompatibleContract),
        DocumentCallError::Failed(_)
    ));
    assert!(matches!(
        classify_sdk_error(ModuleCallError::TransportFailed),
        DocumentCallError::Failed(_)
    ));
}

#[test]
fn the_digest_matches_the_empty_document_vector() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
