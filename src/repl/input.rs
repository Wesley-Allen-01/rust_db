//! Turns REPL input lines into Meta Commands and Statements, and runs the Statements.

use crate::database::Database;
use crate::error::Result;
use crate::execute::{Output, execute};
use crate::lexer::{LexErrorKind, Token, tokenize};

/// A complete piece of input, ready to run.
#[derive(Debug, PartialEq)]
pub enum Input {
    Meta(String),
    /// SQL ending in `;`: one Statement, or several typed together.
    Sql(String),
}

/// Buffers lines until the SQL in them is finished by `;`.
#[derive(Debug, Default)]
pub struct InputBuffer {
    text: String,
}

impl InputBuffer {
    pub fn new() -> Self {
        InputBuffer::default()
    }

    /// Adds a line. Returns a Meta Command at once, or the SQL once it ends with `;`.
    pub fn push(&mut self, line: &str) -> Option<Input> {
        if self.is_empty() && line.trim().is_empty() {
            return None;
        }
        // Like psql, a Meta Command runs at once even mid-Statement, and the
        // unfinished Statement stays buffered. Inside an open string it is text.
        if line.trim_start().starts_with('\\') && !self.in_open_string() {
            return Some(Input::Meta(line.to_string()));
        }
        if !self.is_empty() {
            self.text.push('\n');
        }

        self.text.push_str(line);
        if is_complete(&self.text) {
            Some(Input::Sql(std::mem::take(&mut self.text)))
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

    fn in_open_string(&self) -> bool {
        tokenize(&self.text).is_err_and(|err| err.kind == LexErrorKind::UnterminatedString)
    }
}

/// Runs each Statement in the SQL in turn. Like psql, the first error skips
/// the rest, so it is the last result.
pub fn execute_all(db: &mut Database, sql: &str) -> Vec<Result<Output>> {
    let mut results = Vec::new();
    for statement in split_statements(sql) {
        let result = execute(db, statement);
        let failed = result.is_err();
        results.push(result);
        if failed {
            break;
        }
    }
    results
}

/// Splits complete input into its Statements, each ending at its `;`, as psql
/// does. The lexer finds the `;`s, so one inside a string or comment doesn't
/// split. Empty Statements are dropped. Text that doesn't lex comes back whole,
/// so `execute` reports the error.
pub fn split_statements(sql: &str) -> Vec<&str> {
    let Ok(tokens) = tokenize(sql) else {
        return vec![sql];
    };
    let mut statements = Vec::new();
    // Where the current Statement's first token starts, once it has one.
    let mut start = None;
    for token in tokens {
        if token.token != Token::Semicolon {
            start.get_or_insert(token.offset);
        } else if let Some(start) = start.take() {
            // `;` is one byte, so the Statement ends at its offset.
            statements.push(&sql[start..=token.offset]);
        }
    }
    // A last Statement without `;` still runs: `execute` doesn't need one.
    if let Some(start) = start {
        statements.push(&sql[start..]);
    }
    statements
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
        Some(Input::Sql(text.to_string()))
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
    fn backslash_line_mid_statement_runs_and_keeps_the_statement() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT"), None);
        assert_eq!(buffer.push("\\dt"), Some(Input::Meta("\\dt".to_string())));
        assert!(!buffer.is_empty());
        assert_eq!(buffer.push("1;"), statement("SELECT\n1;"));
    }

    #[test]
    fn backslash_line_inside_an_open_string_is_text() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT 'a"), None);
        assert_eq!(buffer.push("\\dt"), None);
        assert_eq!(buffer.push("';"), statement("SELECT 'a\n\\dt\n';"));
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
        assert_eq!(
            buffer.push("SELECT 1; -- one"),
            statement("SELECT 1; -- one")
        );
    }

    #[test]
    fn lex_errors_other_than_open_strings_are_sent_for_execute_to_report() {
        let mut buffer = InputBuffer::new();
        assert_eq!(buffer.push("SELECT @"), statement("SELECT @"));
    }

    #[test]
    fn splits_a_line_into_its_statements() {
        assert_eq!(
            split_statements("SELECT 1; SELECT 2;"),
            vec!["SELECT 1;", "SELECT 2;"]
        );
    }

    #[test]
    fn split_ignores_semicolons_in_strings_and_comments() {
        assert_eq!(
            split_statements("SELECT 'a;b'; -- c; d\nSELECT 2; -- e;"),
            vec!["SELECT 'a;b';", "SELECT 2;"]
        );
    }

    #[test]
    fn split_keeps_a_multi_line_statement_whole() {
        assert_eq!(
            split_statements("SELECT\n  1;\nSELECT 2;"),
            vec!["SELECT\n  1;", "SELECT 2;"]
        );
    }

    #[test]
    fn split_skips_empty_statements() {
        assert_eq!(split_statements(";; SELECT 1;;"), vec!["SELECT 1;"]);
    }

    #[test]
    fn split_leaves_text_that_does_not_lex_whole_for_execute_to_report() {
        assert_eq!(
            split_statements("SELECT 1; SELECT @"),
            vec!["SELECT 1; SELECT @"]
        );
    }

    #[test]
    fn execute_all_stops_at_the_first_error() {
        let mut db = Database::new();
        let results = execute_all(
            &mut db,
            "CREATE TABLE a (id INT); SELECT x; CREATE TABLE b (id INT);",
        );
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert_eq!(db.table_names(), vec!["a"]);
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
