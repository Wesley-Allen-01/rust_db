//! The REPL binary: reads lines, runs Meta Commands and Statements, prints results.

use rust_db::database::Database;
use rust_db::execute::{Output, execute};
use rust_db::repl::input::StatementBuffer;
use rust_db::repl::meta::{MetaOutcome, run_meta};
use rust_db::repl::render::render_table;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

const PROMPT: &str = "rust_db=> ";
const CONTINUATION_PROMPT: &str = "rust_db-> ";

fn main() -> rustyline::Result<()> {
    let mut editor = DefaultEditor::new()?;
    let mut db = Database::new();
    let mut buffer = StatementBuffer::new();

    loop {
        let prompt = if buffer.is_empty() {
            PROMPT
        } else {
            CONTINUATION_PROMPT
        };

        let line = match editor.readline(prompt) {
            Ok(line) => line,
            // Ctrl-C abandons the unfinished Statement, like psql.
            Err(ReadlineError::Interrupted) => {
                buffer = StatementBuffer::new();
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err),
        };
        if !line.trim().is_empty() {
            editor.add_history_entry(line.as_str())?;
        }

        // Meta Commands only count at the start of a Statement.
        if buffer.is_empty() && line.trim_start().starts_with('\\') {
            match run_meta(&db, &line) {
                Ok(MetaOutcome::Quit) => break,
                Ok(MetaOutcome::Print(text)) => print!("{text}"),
                Err(message) => eprintln!("ERROR: {message}"),
            }
            continue;
        }

        if let Some(sql) = buffer.push(&line) {
            match execute(&mut db, &sql) {
                Ok(Output::Done) => println!("OK"),
                Ok(Output::Rows(result)) => print!("{}", render_table(&result)),
                Err(err) => eprintln!("ERROR: {err}"),
            }
        }
    }

    Ok(())
}
