//! Meta Commands: backslash-prefixed REPL instructions that are not SQL.

use crate::database::Database;
use crate::execute::ResultSet;
use crate::repl::render::render_table;
use crate::value::Value;

/// What the REPL should do after a Meta Command.
#[derive(Debug, PartialEq)]
pub enum MetaOutcome {
    Quit,
    Print(String),
}

/// Runs one Meta Command line (e.g. `\dt`). An error is a message to print.
pub fn run_meta(db: &Database, line: &str) -> Result<MetaOutcome, String> {
    let mut words = line.split_whitespace();
    let command = words.next().unwrap_or("");
    let argument = words.next();

    match (command, argument) {
        ("\\q", None) => Ok(MetaOutcome::Quit),
        ("\\dt", None) => Ok(MetaOutcome::Print(list_tables(db))),
        ("\\d", Some(table)) => describe_table(db, table).map(MetaOutcome::Print),
        ("\\d", None) => Err("usage: \\d <table>".to_string()),
        _ => Err(format!("unknown meta command: {}", line.trim())),
    }
}

fn list_tables(db: &Database) -> String {
    if db.is_empty() {
        return "No tables.\n".to_string();
    }
    let rows = db
        .table_names()
        .into_iter()
        .map(|name| vec![Value::Text(name.to_string())])
        .collect();
    render_table(&ResultSet {
        columns: vec!["Name".to_string()],
        rows,
    })
}

fn describe_table(db: &Database, name: &str) -> Result<String, String> {
    let table = db.get(name).map_err(|err| err.to_string())?;
    let rows = table
        .schema()
        .columns()
        .iter()
        .map(|column| {
            vec![
                Value::Text(column.name().to_string()),
                Value::Text(column.data_type().to_string()),
            ]
        })
        .collect();
    let columns = ResultSet {
        columns: vec!["Column".to_string(), "Type".to_string()],
        rows,
    };
    Ok(format!(
        "Table \"{}\"\n{}",
        table.name(),
        render_table(&columns)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, Schema};
    use crate::value::DataType;

    #[test]
    fn quit_command_quits() {
        assert_eq!(run_meta(&Database::new(), "\\q"), Ok(MetaOutcome::Quit));
    }

    #[test]
    fn unknown_command_is_an_error() {
        assert_eq!(
            run_meta(&Database::new(), "\\x"),
            Err("unknown meta command: \\x".to_string())
        );
    }

    #[test]
    fn list_tables_on_empty_database_says_so() {
        assert_eq!(
            run_meta(&Database::new(), "\\dt"),
            Ok(MetaOutcome::Print("No tables.\n".to_string()))
        );
    }

    fn db_with_tables() -> Database {
        let mut db = Database::new();
        let users = Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("name", DataType::Text),
        ])
        .unwrap();
        let orders = Schema::new(vec![Column::new("total", DataType::Float)]).unwrap();
        db.create_table("users", users).unwrap();
        db.create_table("orders", orders).unwrap();
        db
    }

    fn printed(outcome: Result<MetaOutcome, String>) -> String {
        match outcome {
            Ok(MetaOutcome::Print(text)) => text,
            other => panic!("expected printed output, got {other:?}"),
        }
    }

    #[test]
    fn list_tables_shows_names_in_order() {
        assert_eq!(
            printed(run_meta(&db_with_tables(), "\\dt")),
            "  Name  \n\
             --------\n \
             orders\n \
             users\n\
             (2 rows)\n"
        );
    }

    #[test]
    fn describe_shows_columns_and_data_types() {
        assert_eq!(
            printed(run_meta(&db_with_tables(), "\\d  USERS ")),
            "Table \"users\"\n \
             Column | Type \n\
             --------+------\n \
             id     | INT\n \
             name   | TEXT\n\
             (2 rows)\n"
        );
    }

    #[test]
    fn describe_unknown_table_is_an_error() {
        assert_eq!(
            run_meta(&db_with_tables(), "\\d nope"),
            Err("no such table `nope`".to_string())
        );
    }

    #[test]
    fn describe_without_a_table_name_is_an_error() {
        assert_eq!(
            run_meta(&db_with_tables(), "\\d"),
            Err("usage: \\d <table>".to_string())
        );
    }
}
