use super::xml_text;

#[test]
fn unknown_entities_do_not_add_text() {
    assert_eq!(
        xml_text("<a:p><a:t>before&unknown;after</a:t></a:p>", "a:p", "a:t"),
        "beforeafter\n\n"
    );
}

#[test]
fn malformed_xml_keeps_text_read_before_the_error() {
    assert_eq!(
        xml_text("<a:p><a:t>before</a:t><broken", "a:p", "a:t"),
        "before"
    );
}
