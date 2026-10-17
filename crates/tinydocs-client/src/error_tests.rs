//! Formatting behavior for TinyDocs errors, including message redaction.

use super::*;

#[test]
fn debug_redacts_every_module_message_but_display_keeps_the_intended_input_reason() {
    let errors = [
        DocumentCallError::Unavailable("module secret".to_string()),
        DocumentCallError::InvalidInput("field must be present".to_string()),
        DocumentCallError::Failed("remote detail".to_string()),
    ];
    for error in errors {
        let debug = format!("{error:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("present"));
        assert!(!debug.contains("remote detail"));
    }
    assert_eq!(
        DocumentCallError::InvalidInput("field must be present".to_string()).to_string(),
        "field must be present"
    );
}
