//! The library's entry point for SQL: one Statement in, an Output or an error out.

use crate::ast::DropTable;
use crate::binder::{BoundCreateTable, BoundSelect, BoundStatement, bind};
use crate::database::Database;
use crate::error::{DbError, Result};
use crate::parser::parse;
use crate::planner::plan_select;
use crate::schema::ResultColumn;
use crate::value::Value;

/// What a successful Statement produces.
#[derive(Debug, Clone)]
pub enum Output {
    /// The Statement ran and returns no Rows (e.g. CREATE TABLE, INSERT).
    Done {
        /// psql-style command tag, e.g. "CREATE TABLE", "INSERT 0 2", "UPDATE 3".
        tag: String,
        /// Rows inserted, updated or deleted; 0 for Statements that change no Rows.
        rows_affected: u64,
    },
    /// The Statement returns Rows (e.g. SELECT).
    Rows(ResultSet),
}

/// Rows returned by a Statement, with the names and Data Types of their Columns.
///
/// Rows are plain `Vec<Value>` rather than `table::Row`, because a result Row
/// (e.g. a projection or an aggregate) need not belong to any Table.
#[derive(Debug, Clone)]
pub struct ResultSet {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
}

impl ResultSet {
    /// Each Row's Values as display text.
    pub fn text_rows(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|row| row.iter().map(|value| value.to_string()).collect())
            .collect()
    }
}

/// Runs exactly one SQL Statement against the Database.
///
/// The Statement may end with a `;`. Anything after that first `;` (such as a
/// second Statement) is an error, not silently ignored; the parser enforces this.
/// For now that means a REPL line holding several Statements is rejected too.
pub fn execute(db: &mut Database, sql: &str) -> Result<Output> {
    let statement = parse(sql)?;
    match bind(db, statement)? {
        BoundStatement::Select(select) => run_select(select).map(Output::Rows),
        BoundStatement::CreateTable(create) => {
            create_table(db, create)?;
            Ok(done("CREATE TABLE"))
        }
        BoundStatement::DropTable(drop) => {
            drop_table(db, drop)?;
            Ok(done("DROP TABLE"))
        }
    }
}

/// The Output of a Statement that changes no Rows.
fn done(tag: &str) -> Output {
    Output::Done {
        tag: tag.to_string(),
        rows_affected: 0,
    }
}

/// `IF NOT EXISTS` turns "already exists" into a no-op, as in Postgres.
fn create_table(db: &mut Database, create: BoundCreateTable) -> Result<()> {
    match db.create_table(create.name, create.schema) {
        Err(DbError::TableAlreadyExists { .. }) if create.if_not_exists => Ok(()),
        result => result,
    }
}

/// `IF EXISTS` turns "no such table" into a no-op, as in Postgres.
fn drop_table(db: &mut Database, drop: DropTable) -> Result<()> {
    match db.drop_table(&drop.name) {
        Err(DbError::NoSuchTable { .. }) if drop.if_exists => Ok(()),
        result => result.map(|_| ()),
    }
}

fn run_select(select: BoundSelect) -> Result<ResultSet> {
    let columns = select.columns.clone();
    let mut plan = plan_select(select);
    let mut rows = Vec::new();
    while let Some(row) = plan.next()? {
        rows.push(row);
    }
    Ok(ResultSet { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DataType;

    fn tag(db: &mut Database, sql: &str) -> String {
        match execute(db, sql).unwrap() {
            Output::Done { tag, .. } => tag,
            other => panic!("expected Done, got {other:?}"),
        }
    }

    // Here rather than in slt, which discards the tag.
    #[test]
    fn create_and_drop_report_psql_status_tags() {
        let mut db = Database::new();
        assert_eq!(tag(&mut db, "CREATE TABLE t (id INT)"), "CREATE TABLE");
        assert_eq!(tag(&mut db, "DROP TABLE t"), "DROP TABLE");
    }

    // Here rather than in slt, which can't check Column names.
    #[test]
    fn as_names_a_result_column() {
        let Output::Rows(result) = execute(&mut Database::new(), "SELECT 1 AS one, 'a'").unwrap()
        else {
            panic!("expected Rows");
        };
        assert_eq!(
            result.columns,
            vec![
                ResultColumn::new("one", DataType::Int),
                // Postgres's name for an unnamed expression.
                ResultColumn::new("?column?", DataType::Text),
            ]
        );
    }
}
