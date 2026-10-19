//! Meta Commands: backslash-prefixed REPL instructions that are not SQL.

use crate::database::Database;
use crate::execute::{ResultColumn, ResultSet};
use crate::repl::render::render_table;
use crate::value::{DataType, Value};

/// What the REPL should do after a Meta Command.
#[derive(Debug, PartialEq)]
pub enum MetaOutcome {
    Quit,
    Print(String),
}

/// Runs one Meta Command line (e.g. `\dt`). An error is a message to print.
pub fn run_meta(db: &Database, line: &str) -> Result<MetaOutcome, String> {
    let words: Vec<&str> = line.split_whitespace().collect();

    match words.as_slice() {
        ["\\q"] => Ok(MetaOutcome::Quit),
        ["\\dt"] => Ok(MetaOutcome::Print(list_tables(db))),
        ["\\d", table] => describe_table(db, table).map(MetaOutcome::Print),
        [command @ ("\\q" | "\\dt"), ..] => Err(format!("{command} takes no arguments")),
        ["\\d", ..] => Err("usage: \\d <table>".to_string()),
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
        columns: vec![ResultColumn::new("Name", DataType::Text)],
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
    let listing = ResultSet {
        columns: vec![
            ResultColumn::new("Column", DataType::Text),
            ResultColumn::new("Type", DataType::Text),
        ],
        rows,
    };
    Ok(format!(
        "Table \"{}\"\n{}",
        table.name(),
        render_table(&listing)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, Schema};

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
    fn describe_without_exactly_one_table_name_is_an_error() {
        for line in ["\\d", "\\d users orders"] {
            assert_eq!(
                run_meta(&db_with_tables(), line),
                Err("usage: \\d <table>".to_string())
            );
        }
    }

    #[test]
    fn commands_without_arguments_reject_extra_words() {
        assert_eq!(
            run_meta(&db_with_tables(), "\\dt users"),
            Err("\\dt takes no arguments".to_string())
        );
        assert_eq!(
            run_meta(&db_with_tables(), "\\q now"),
            Err("\\q takes no arguments".to_string())
        );
    }
}
