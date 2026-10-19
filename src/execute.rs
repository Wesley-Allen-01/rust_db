//! The library's entry point for SQL: one Statement in, an Output or an error out.

use crate::binder::{BoundInsert, BoundSelect, BoundStatement, bind};
use crate::database::Database;
use crate::error::Result;
use crate::eval::eval;
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
        // The binder has checked the name; the Database checks it again as a guard.
        BoundStatement::CreateTable(create) => {
            db.create_table(create.name, create.schema)?;
            Ok(done("CREATE TABLE"))
        }
        BoundStatement::DropTable(drop) => {
            db.drop_table(&drop.name)?;
            Ok(done("DROP TABLE"))
        }
        BoundStatement::Insert(insert) => run_insert(db, insert),
        BoundStatement::NoOp { tag } => Ok(done(tag)),
    }
}

/// Evaluates every row before inserting any, and the Table inserts them all
/// or none, so a failing row leaves the Table unchanged.
fn run_insert(db: &mut Database, insert: BoundInsert) -> Result<Output> {
    let rows = insert
        .rows
        .iter()
        .map(|row| row.iter().map(eval).collect::<Result<Vec<Value>>>())
        .collect::<Result<Vec<_>>>()?;
    let count = db.get_mut(&insert.table)?.insert_all(rows)?.len() as u64;
    Ok(Output::Done {
        // The 0 is the OID psql reports; it is always 0 in modern Postgres.
        tag: format!("INSERT 0 {count}"),
        rows_affected: count,
    })
}

/// The Output of a Statement that changes no Rows.
fn done(tag: &str) -> Output {
    Output::Done {
        tag: tag.to_string(),
        rows_affected: 0,
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

    #[test]
    fn insert_reports_a_psql_status_tag_and_row_count() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT)").unwrap();
        let output = execute(&mut db, "INSERT INTO t VALUES (1), (2)").unwrap();
        let Output::Done { tag, rows_affected } = output else {
            panic!("expected Done");
        };
        assert_eq!(tag, "INSERT 0 2");
        assert_eq!(rows_affected, 2);
    }

    // Checked through Table until SELECT ... FROM can show the Rows.
    #[test]
    fn a_failing_last_row_leaves_the_table_empty() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT NOT NULL)").unwrap();
        for sql in [
            "INSERT INTO t VALUES (1), (2), (NULL)",
            "INSERT INTO t VALUES (1), (2), (1 / 0)",
        ] {
            assert!(execute(&mut db, sql).is_err(), "{sql}");
            assert_eq!(db.get("t").unwrap().len(), 0, "{sql}");
        }
    }

    #[test]
    fn insert_stores_rows_in_schema_order_with_coercion() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT, name TEXT, score FLOAT)").unwrap();
        execute(&mut db, "INSERT INTO t (score, id) VALUES (3, 1)").unwrap();
        let rows: Vec<&[Value]> = db.get("t").unwrap().iter().map(|r| r.values()).collect();
        assert_eq!(
            rows,
            vec![&[Value::Int(1), Value::Null, Value::Float(3.0)][..]]
        );
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
