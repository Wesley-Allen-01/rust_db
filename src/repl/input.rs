//! Buffers REPL input lines until a Statement is finished by `;`.

#[derive(Debug, Default)]
pub struct StatementBuffer {
    text: String,
}

impl StatementBuffer {
    pub fn new() -> Self {
        StatementBuffer::default()
    }

    /// Adds a line. Returns the buffered Statement once the input ends with `;`.
    pub fn push(&mut self, line: &str) -> Option<String> {
        if self.is_empty() && line.trim().is_empty() {
            return None;
        }
        if !self.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(line);
        if self.text.trim_end().ends_with(';') {
            Some(std::mem::take(&mut self.text))
        } else {
            None
        }
    }

    /// True when no unfinished Statement is waiting for more lines.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_statement_completes_at_semicolon() {
        let mut buffer = StatementBuffer::new();
        assert_eq!(buffer.push("SELECT 1;"), Some("SELECT 1;".to_string()));
    }

    #[test]
    fn multi_line_statement_is_buffered_until_semicolon() {
        let mut buffer = StatementBuffer::new();
        assert_eq!(buffer.push("SELECT"), None);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.push("  1"), None);
        assert_eq!(buffer.push(";"), Some("SELECT\n  1\n;".to_string()));
        assert!(buffer.is_empty());
    }

    #[test]
    fn blank_lines_before_a_statement_are_ignored() {
        let mut buffer = StatementBuffer::new();
        assert_eq!(buffer.push("   "), None);
        assert!(buffer.is_empty());
    }
}
