//! UTF-8-safe bounded formatting shared by retained I/O context and dumps.
use std::fmt;
pub(crate) struct BoundedText {
    pub(crate) text: String,
    pub(crate) truncated: bool,
    limit: usize,
}
impl BoundedText {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            truncated: false,
            limit,
        }
    }
}
impl fmt::Write for BoundedText {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let mut end = input.len().min(self.limit - self.text.len());
        while !input.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&input[..end]);
        if end < input.len() {
            self.truncated = true;
            return Err(fmt::Error);
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "diagnostic_text_test.rs"]
mod diagnostic_text_test;
