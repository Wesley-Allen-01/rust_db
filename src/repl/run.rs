//! Runs the SQL the REPL reads, one Statement at a time.

use crate::database::Database;
use crate::error::Result;
use crate::execute::{Output, execute};
use crate::repl::input::split_statements;

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
}
