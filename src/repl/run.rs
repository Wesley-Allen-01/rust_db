//! Runs the SQL the REPL reads, one Statement at a time.

use crate::database::Database;
use crate::error::{DbError, Result};
use crate::execute::{Output, execute};
use crate::lexer::Position;
use crate::repl::input::split_statements;

/// Runs each Statement in the SQL in turn. Like psql, the first error skips
/// the rest, so it is the last result. Error positions count from the start of
/// the SQL, not of the Statement.
pub fn execute_all(db: &mut Database, sql: &str) -> Vec<Result<Output>> {
    let mut results = Vec::new();
    for statement in split_statements(sql) {
        let result = execute(db, statement.text).map_err(|err| shift(err, statement.start));
        let failed = result.is_err();
        results.push(result);
        if failed {
            break;
        }
    }
    results
}

/// Moves a syntax error's position, counted within a Statement, to count
/// within the input where the Statement starts at `start`.
fn shift(err: DbError, start: Position) -> DbError {
    let DbError::Syntax { pos, message } = err else {
        return err;
    };
    // Only the Statement's first line begins part way along an input line.
    let col_shift = if pos.line == 1 { start.col - 1 } else { 0 };
    let pos = Position {
        line: pos.line + start.line - 1,
        col: pos.col + col_shift,
    };
    DbError::Syntax { pos, message }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Where the last result's syntax error points, as (line, col).
    fn error_at(sql: &str) -> (usize, usize) {
        match execute_all(&mut Database::new(), sql).pop() {
            Some(Err(DbError::Syntax { pos, .. })) => (pos.line, pos.col),
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn error_positions_count_from_the_start_of_the_input() {
        assert_eq!(error_at("SELECT 1; SELECT 2 +;"), (1, 21));
        assert_eq!(error_at("SELECT 1;\nSELECT 1; SELECT 2 +;"), (2, 21));
        // Past the Statement's first line, the column needs no shift.
        assert_eq!(error_at("SELECT 1; SELECT\n  2 +;"), (2, 6));
    }

    #[test]
    fn statements_before_a_lex_error_still_run() {
        let mut db = Database::new();
        let results = execute_all(&mut db, "CREATE TABLE a (id INT); SELECT \"x\";");
        assert_eq!(db.table_names(), vec!["a"]);
        assert!(matches!(
            results.as_slice(),
            [
                Ok(_),
                Err(DbError::Syntax {
                    pos: Position { line: 1, col: 33 },
                    ..
                })
            ]
        ));
    }
}
