//! The REPL binary: reads lines, runs Meta Commands and Statements, prints results.

use rust_db::database::Database;
use rust_db::repl::input::{Input, InputBuffer};
use rust_db::repl::meta::{MetaOutcome, run_meta};
use rust_db::repl::render::render_output;
use rust_db::repl::run::execute_all;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

const PROMPT: &str = "rust_db=> ";
const CONTINUATION_PROMPT: &str = "rust_db-> ";

fn main() -> rustyline::Result<()> {
    let mut editor = DefaultEditor::new()?;
    let mut db = Database::new();
    let mut buffer = InputBuffer::new();

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
                buffer.clear();
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err),
        };
        // History holds whole Meta Commands and Statements, so Up recalls a
        // multi-line Statement in one piece, like psql.
        match buffer.push(&line) {
            None => {}
            Some(Input::Meta(command)) => {
                editor.add_history_entry(command.as_str())?;
                match run_meta(&db, &command) {
                    Ok(MetaOutcome::Quit) => break,
                    Ok(MetaOutcome::Print(text)) => print!("{text}"),
                    Err(message) => eprintln!("ERROR: {message}"),
                }
            }
            Some(Input::Sql(sql)) => {
                editor.add_history_entry(sql.as_str())?;
                for result in execute_all(&mut db, &sql) {
                    match result {
                        Ok(output) => print!("{}", render_output(&output)),
                        Err(err) => eprintln!("ERROR: {err}"),
                    }
                }
            }
        }
    }

    Ok(())
}
