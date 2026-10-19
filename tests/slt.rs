//! Runs every `tests/slt/**/*.slt` file against the library's execute API.

use rust_db::database::Database;
use rust_db::error::DbError;
use rust_db::execute::{Output, execute};
use sqllogictest::{DBOutput, DefaultColumnType, Runner};
use std::fs;
use std::path::{Path, PathBuf};

struct Engine {
    db: Database,
}

impl sqllogictest::DB for Engine {
    type Error = DbError;
    type ColumnType = DefaultColumnType;

    fn run(&mut self, sql: &str) -> Result<DBOutput<DefaultColumnType>, DbError> {
        match execute(&mut self.db, sql)? {
            Output::Done { rows_affected, .. } => Ok(DBOutput::StatementComplete(rows_affected)),
            Output::Rows(result) => Ok(DBOutput::Rows {
                // Column types are not checked yet, so report every column as Any.
                types: vec![DefaultColumnType::Any; result.columns.len()],
                rows: result
                    .text_rows()
                    .into_iter()
                    .map(|row| row.into_iter().map(slt_text).collect())
                    .collect(),
            }),
        }
    }
}

/// sqllogictest can't express an empty string in a results block, so it spells it `(empty)`.
fn slt_text(text: String) -> String {
    if text.is_empty() {
        "(empty)".to_string()
    } else {
        text
    }
}

fn slt_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("tests/slt should exist") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(slt_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "slt") {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn slt_files_pass() {
    let files = slt_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/slt"));
    assert!(!files.is_empty(), "no .slt files found under tests/slt");

    for file in files {
        // A fresh Database per file keeps files independent of each other.
        let mut runner = Runner::new(|| async {
            Ok(Engine {
                db: Database::new(),
            })
        });
        if let Err(err) = runner.run_file(&file) {
            panic!("{}: {err}", file.display());
        }
    }
}
