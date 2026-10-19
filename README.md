# rust_db

A SQL database built for learning depth. The parser, executor and storage engine are hand-written, behind a psql-style REPL. Milestone 1 keeps every Table in memory, so nothing survives a restart.

## Running the REPL

```sh
cargo run
```

Type SQL at the `rust_db=>` prompt. A Statement runs once it ends with `;`, so it can span several lines; the prompt changes to `rust_db->` while one is unfinished. Several Statements on one line run in order, and the first error skips the rest.

Ctrl-C abandons an unfinished Statement. `\q` or Ctrl-D quits.

| Meta Command | Does |
| --- | --- |
| `\dt` | Lists the Tables |
| `\d <table>` | Describes a Table's Columns |
| `\q` | Quits |

A Meta Command typed mid-Statement runs at once, and the unfinished Statement stays buffered.

## Supported SQL

- `CREATE TABLE [IF NOT EXISTS] t (col TYPE [NOT NULL], ...)` and `DROP TABLE [IF EXISTS] t`
- `INSERT INTO t [(col, ...)] VALUES (...), (...)`
- `SELECT expr [AS name], ... | * [FROM t] [WHERE ...] [ORDER BY expr [ASC|DESC], ...] [LIMIT n] [OFFSET m]`
- `UPDATE t SET col = expr, ... [WHERE ...]` and `DELETE FROM t [WHERE ...]`

Data Types are `INT`, `FLOAT`, `TEXT` and `BOOL`. Expressions support `+ - * / %`, `||`, `= <> < <= > >=`, `AND OR NOT` and `IS [NOT] NULL`.

Typing is strict, as in Postgres: the only Coercion is INT to FLOAT, and any other mismatch is an error. NULL follows three-valued logic, and WHERE keeps a Row only when it is TRUE. Each Statement takes full effect or none. Names are case-insensitive, and `--` starts a comment.

## Example session

```
rust_db=> CREATE TABLE users (id INT NOT NULL, name TEXT, score FLOAT);
CREATE TABLE
rust_db=> INSERT INTO users VALUES (1, 'ada', 9.5), (2, 'bob', NULL), (3, 'cy', 7);
INSERT 0 3
rust_db=> SELECT name, score FROM users WHERE score > 8 OR score IS NULL ORDER BY name;
 name | score 
------+-------
 ada  |   9.5
 bob  | NULL
(2 rows)
rust_db=> UPDATE users SET score = score + 1 WHERE id = 3;
UPDATE 1
rust_db=> DELETE FROM users WHERE name = 'bob';
DELETE 1
rust_db=> SELECT * FROM users WHERE id > 10;
 id | name | score 
----+------+-------
(0 rows)
rust_db=> \d users
Table "users"
 Column | Type  | Nullable 
--------+-------+----------
 id     | INT   | not null
 name   | TEXT  |
 score  | FLOAT |
(3 rows)
rust_db=> SELECT name FROM users WHERE score = 'high';
ERROR: operator = cannot be applied to FLOAT and TEXT
```

## Development

`cargo test` runs the unit tests and the sqllogictest files under `tests/slt/`. See `GLOSSARY.md` for the domain terms and `docs/adr/` for the design decisions.
