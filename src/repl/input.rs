//! Turns REPL input lines into Meta Commands and Statements.

use crate::lexer::{LexErrorKind, Position, Token, tokenize};

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

/// One Statement's text, cut from the REPL's input.
#[derive(Debug, PartialEq)]
pub struct StatementText<'a> {
    pub text: &'a str,
    /// Where `text` starts in the input.
    pub start: Position,
}

/// Splits complete input into its Statements, each ending at its `;`, as psql
/// does. The lexer finds the `;`s, so one inside a string or comment doesn't
/// split. Empty Statements are dropped. After a lex error nothing more can be
/// split, so the rest of the input, from the start of the Statement holding
/// the error, comes back as one piece for `execute` to report.
pub fn split_statements(sql: &str) -> Vec<StatementText<'_>> {
    let (tokens, lex_error) = match tokenize(sql) {
        Ok(tokens) => (tokens, None),
        Err(err) => {
            // The lexer reads left to right, so it got this far cleanly.
            let before = tokenize(&sql[..err.offset]).expect("text before a lex error lexes");
            (before, Some(err))
        }
    };
    let mut statements = Vec::new();
    // Where the current Statement's first token starts, once it has one.
    let mut start = None;
    for token in tokens {
        if token.token != Token::Semicolon {
            start.get_or_insert((token.offset, token.pos));
        } else if let Some((offset, pos)) = start.take() {
            // `;` is one byte, so the Statement ends at its offset.
            statements.push(StatementText {
                text: &sql[offset..=token.offset],
                start: pos,
            });
        }
    }
    // What's left runs too: a last Statement without `;`, which `execute`
    // accepts, or the text with the lex error in it.
    if let Some((offset, pos)) = start.or(lex_error.map(|err| (err.offset, err.pos))) {
        statements.push(StatementText {
            text: &sql[offset..],
            start: pos,
        });
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

    /// The text of each Statement `split_statements` finds.
    fn split(sql: &str) -> Vec<&str> {
        split_statements(sql).iter().map(|s| s.text).collect()
    }

    #[test]
    fn splits_a_line_into_its_statements() {
        assert_eq!(split("SELECT 1; SELECT 2;"), vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn split_ignores_semicolons_in_strings_and_comments() {
        assert_eq!(
            split("SELECT 'a;b'; -- c; d\nSELECT 2; -- e;"),
            vec!["SELECT 'a;b';", "SELECT 2;"]
        );
    }

    #[test]
    fn split_keeps_a_multi_line_statement_whole() {
        assert_eq!(
            split("SELECT\n  1;\nSELECT 2;"),
            vec!["SELECT\n  1;", "SELECT 2;"]
        );
    }

    #[test]
    fn split_skips_empty_statements() {
        assert_eq!(split(";; SELECT 1;;"), vec!["SELECT 1;"]);
    }

    #[test]
    fn split_reports_where_each_statement_starts() {
        let starts: Vec<Position> = split_statements("SELECT 1; SELECT 2;\n  SELECT 3;")
            .iter()
            .map(|s| s.start)
            .collect();
        let at = |line, col| Position { line, col };
        assert_eq!(starts, vec![at(1, 1), at(1, 11), at(2, 3)]);
    }

    #[test]
    fn split_keeps_the_statements_before_a_lex_error() {
        // Everything from the Statement holding the error on goes to `execute`
        // to report.
        assert_eq!(
            split("CREATE TABLE a (id INT); SELECT \"x\"; SELECT 2;"),
            vec!["CREATE TABLE a (id INT);", "SELECT \"x\"; SELECT 2;"]
        );
        assert_eq!(split("SELECT 1; SELECT @"), vec!["SELECT 1;", "SELECT @"]);
    }

    #[test]
    fn split_sends_a_meta_command_after_sql_as_sql() {
        // Unlike psql, `\dt` here is SQL text, so `execute` rejects it.
        assert_eq!(split("SELECT 1; \\dt"), vec!["SELECT 1;", "\\dt"]);
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
