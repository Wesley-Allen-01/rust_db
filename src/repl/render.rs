//! psql-style text for showing a Statement's Output.

use crate::execute::{Output, ResultSet};
use crate::value::Value;

/// A Done Statement shows its command tag; a Statement with Rows shows a table.
pub fn render_output(output: &Output) -> String {
    match output {
        Output::Done { tag, .. } => format!("{tag}\n"),
        Output::Rows(result) => render_table(result),
    }
}

/// Renders Rows like psql: centred headers, a dashed rule, then a row-count footer.
pub fn render_table(result: &ResultSet) -> String {
    let text_rows = result.text_rows();
    let widths = column_widths(&result.columns, &text_rows);

    let mut out = String::new();
    let header: Vec<String> = result
        .columns
        .iter()
        .zip(&widths)
        .map(|(name, &width)| centre(name, width))
        .collect();
    out.push_str(&format!(" {} \n", header.join(" | ")));

    let rule: Vec<String> = widths.iter().map(|&w| "-".repeat(w + 2)).collect();
    out.push_str(&rule.join("+"));
    out.push('\n');

    for (row, texts) in result.rows.iter().zip(&text_rows) {
        let padded: Vec<String> = row
            .iter()
            .zip(texts)
            .zip(&widths)
            .map(|((value, text), &width)| pad(value, text, width))
            .collect();
        // Like psql, data lines carry no trailing padding.
        out.push_str(&format!(" {}", padded.join(" | ").trim_end()));
        out.push('\n');
    }

    out.push_str(&footer(text_rows.len()));
    out
}

/// Numbers sit to the right of their column, everything else to the left.
fn pad(value: &Value, text: &str, width: usize) -> String {
    match value {
        Value::Int(_) | Value::Float(_) => format!("{text:>width$}"),
        _ => format!("{text:<width$}"),
    }
}

fn column_widths(columns: &[String], text_rows: &[Vec<String>]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let widest_text = text_rows
                .iter()
                .map(|row| row[i].chars().count())
                .max()
                .unwrap_or(0);
            name.chars().count().max(widest_text)
        })
        .collect()
}

fn centre(text: &str, width: usize) -> String {
    let spare = width - text.chars().count();
    let left = spare / 2;
    format!("{}{text}{}", " ".repeat(left), " ".repeat(spare - left))
}

fn footer(count: usize) -> String {
    let noun = if count == 1 { "row" } else { "rows" };
    format!("({count} {noun})\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(columns: &[&str], rows: Vec<Vec<Value>>) -> ResultSet {
        ResultSet {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        }
    }

    #[test]
    fn renders_text_column_with_row_count_footer() {
        let out = render_table(&result(
            &["name"],
            vec![
                vec![Value::Text("alice".into())],
                vec![Value::Text("bob".into())],
            ],
        ));
        assert_eq!(
            out,
            " name  \n\
             -------\n \
             alice\n \
             bob\n\
             (2 rows)\n"
        );
    }

    #[test]
    fn right_aligns_numbers_and_widens_to_longest_value() {
        let out = render_table(&result(
            &["id", "price"],
            vec![
                vec![Value::Int(7), Value::Float(2.5)],
                vec![Value::Int(1234), Value::Float(10.25)],
            ],
        ));
        assert_eq!(
            out,
            "  id  | price \n\
             ------+-------\n    \
                 7 |   2.5\n \
             1234 | 10.25\n\
             (2 rows)\n"
        );
    }

    #[test]
    fn shows_null_as_the_word_null() {
        // Unlike psql's blank default, NULL stays distinguishable from empty TEXT.
        let out = render_table(&result(
            &["n", "t"],
            vec![vec![Value::Null, Value::Text(String::new())]],
        ));
        assert_eq!(
            out,
            "  n   | t \n\
             ------+---\n \
             NULL |\n\
             (1 row)\n"
        );
    }

    #[test]
    fn renders_empty_result_with_zero_rows_footer() {
        let out = render_table(&result(&["id"], vec![]));
        assert_eq!(out, " id \n----\n(0 rows)\n");
    }

    #[test]
    fn done_output_shows_its_command_tag() {
        let done = Output::Done {
            tag: "INSERT 0 2".to_string(),
            rows_affected: 2,
        };
        assert_eq!(render_output(&done), "INSERT 0 2\n");
    }

    #[test]
    fn rows_output_shows_a_table() {
        let rows = Output::Rows(result(&["id"], vec![vec![Value::Int(1)]]));
        assert_eq!(render_output(&rows), " id \n----\n  1\n(1 row)\n");
    }

    #[test]
    fn measures_width_in_characters_not_bytes() {
        let out = render_table(&result(&["city"], vec![vec![Value::Text("Zürich".into())]]));
        assert_eq!(out, "  city  \n--------\n Zürich\n(1 row)\n");
    }
}
