# Testing

All work is test-driven with the `tdd` skill: red → green, one vertical slice at a time.

## Agreed seams

The `tdd` skill requires seams to be agreed before writing tests. These are pre-agreed, so you can write tests against them without asking the user. A new seam (e.g. the disk format in M2) gets agreed during that milestone's `/grill-with-docs` session and added here.

| Seam | Where tests live | Catches | Misses |
| --- | --- | --- | --- |
| **SQL**: Statements in, results or errors out, through the library's public execute API | `tests/slt/**/*.slt` (sqllogictest) | End-to-end SQL behaviour, error cases, three-valued logic | Internal structure, REPL formatting |
| **Module interfaces**: the public API of the lexer, parser, binder, `Value`, `Schema`, `Table`, `Database`, and each Operator | `#[cfg(test)] mod tests` in the module | Edge cases that are awkward to reach through SQL, precise error variants | How modules interact |
| **REPL helpers**: table rendering and Meta Command parsing as pure functions | `#[cfg(test)] mod tests` in the REPL module | Output layout, Meta Command handling | Terminal I/O itself (left untested) |

Prefer the SQL seam whenever the behaviour can be reached from SQL. Use module tests for what SQL can't reach cleanly.
