use super::*;
use std::fmt::Write;
#[test]
fn unicode_truncation_respects_byte_limit() {
    let mut output = BoundedText::new(5);
    assert!(write!(output, "abcdé").is_err());
    assert_eq!(output.text, "abcd");
    assert!(output.truncated);
}
