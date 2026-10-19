//! Turns REPL input lines into complete Meta Commands and Statements.

use crate::lexer::{LexErrorKind, Token, tokenize};

/// A complete piece of input, ready to run.
#[derive(Debug, PartialEq)]
pub enum Input {
    Meta(String),
    Statement(String),
}

/// Buffers lines until a Statement is finished by `;`.
#[derive(Debug, Default)]
pub struct InputBuffer {
    text: String,
}

impl InputBuffer {
    pub fn new() -> Self {
        InputBuffer::default()
    }

    /// Adds a line. Returns a Meta Command at once, or a Statement once it ends with `;`.
    pub fn push(&mut self, line: &str) -> Option<Input> {
        if self.is_empty() {
            if line.trim().is_empty() {
                return None;
            }
            // Meta Commands only count at the start of a Statement; mid-Statement a
            // backslash is just more SQL text.
            if line.trim_start().starts_with('\\') {
                return Some(Input::Meta(line.to_string()));
            }
        } else {
            self.text.push('\n');
        }

        self.text.push_str(line);
        if is_complete(&self.text) {
            Some(Input::Statement(std::mem::take(&mut self.text)))
        } else {
            None
        }
    }

    /// True when no unfinished Statement is waiting for more lines.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Abandons the unfinished Statement.
    pub fn clear(&mut self) {
        self.text.clear();
    }
}

/// The lexer, not a raw `ends_with(';')`, decides, so a `;` inside a string or a
/// `--` comment doesn't end the Statement. Any lex error except an open string is
/// final too: more lines can't fix it, so `execute` gets to report it.
fn is_complete(sql: &str) -> bool {
    match tokenize(sql) {
        Ok(tokens) => tokens
            .last()
            .is_some_and(|last| last.token == Token::Semicolon),
        Err(err) => err.kind != LexErrorKind::UnterminatedString,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement(text: &str) -> Option<Input> {
        Some(Input::Statement(text.to_string()))
    }

    #[test]
    fn single_line_statement_completes_at_semicolon() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT 1;"), statement("SELECT 1;"));
    }

    #[test]
    fn multi_line_statement_is_buffered_until_semicolon() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT"), None);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.push("  1"), None);
        assert_eq!(buffer.push(";"), statement("SELECT\n  1\n;"));
        assert!(buffer.is_empty());
    }

    #[test]
    fn blank_lines_before_a_statement_are_ignored() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("   "), None);
        assert!(buffer.is_empty());
    }

    #[test]
    fn backslash_line_at_start_is_a_meta_command() {
        let mut buffer = InputBuffer::new();
        assert_eq!(
            buffer.push("  \\dt"),
            Some(Input::Meta("  \\dt".to_string()))
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn backslash_line_mid_statement_is_sql_text() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT"), None);
        // `\` can't start a SQL token, so the Statement goes to `execute` to report.
        assert_eq!(buffer.push("\\dt"), statement("SELECT\n\\dt"));
    }

    #[test]
    fn semicolon_inside_a_string_does_not_end_the_statement() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("INSERT INTO t VALUES ('a;"), None);
        assert_eq!(
            buffer.push("b');"),
            statement("INSERT INTO t VALUES ('a;\nb');")
        );
    }

    #[test]
    fn semicolon_inside_a_comment_does_not_end_the_statement() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT 1 -- note;"), None);
    }

    #[test]
    fn trailing_comment_after_semicolon_still_ends_the_statement() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT 1; -- one"), statement("SELECT 1; -- one"));
    }

    #[test]
    fn lex_errors_other_than_open_strings_are_sent_for_execute_to_report() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT @"), statement("SELECT @"));
    }

    #[test]
    fn clear_abandons_the_unfinished_statement() {
        let mut buffer = InputBuffer::new();
        buffer.push("SELECT");
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.push("SELECT 2;"), statement("SELECT 2;"));
    }
}
