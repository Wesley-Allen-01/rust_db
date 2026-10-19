# rust_db

A SQL database built for learning depth: every core layer (parsing, execution, storage) is hand-written, driven from an interactive SQL REPL.

## Data

**Value**:
A single piece of data held in a Row: an integer, float, text, boolean, or NULL.
_Avoid_: cell, field, datum

**Data Type**:
The kind of Value a Column holds: INT, FLOAT, TEXT, or BOOL.
_Avoid_: column type, kind

**NULL**:
The absence of a Value. Comparing anything to NULL yields unknown rather than true or false, so `NULL = NULL` is not true.
_Avoid_: nil, none, empty

**Coercion**:
The automatic conversion of a Value to another Data Type. The only Coercion is INT to FLOAT; every other Data Type mismatch is an error.
_Avoid_: cast, conversion, promotion

## Structure

**Database**:
A named collection of Tables.

**Table**:
A named collection of Rows that all conform to one Schema.
_Avoid_: relation, collection

**Schema**:
The ordered list of Columns that defines a Table's shape.
_Avoid_: definition, layout

**Column**:
A named, typed slot in a Schema. Column names are case-insensitive.
_Avoid_: field, attribute

**Row**:
The unit of data a Table holds: an ordered list of Values, one per Column.
_Avoid_: record, tuple, entry

**Constraint**:
A rule on a Column that every Row must satisfy beyond its Data Type, such as NOT NULL.

**Row ID**:
The Table's own permanent handle for a Row, assigned on insert and never reused. Users never set it, and it is distinct from any user-defined key column.
_Avoid_: id, rowid, key, primary key

## Querying

**Statement**:
One complete SQL command, such as a SELECT or an INSERT. A Statement either takes full effect or none at all.
_Avoid_: command, query (when not a SELECT)

**Result Set**:
The Columns and Rows a Statement such as a SELECT returns. Each Column has a name and a Data Type, known before any Row is produced. A Column that can only ever hold NULL is TEXT. Its Rows need not belong to any Table.
_Avoid_: result, output, rowset

**Plan**:
The tree of Operators that a Statement is turned into before it runs.
_Avoid_: query plan, execution plan

**Operator**:
One node of a Plan that produces Rows on demand, pulling them from its child Operators (e.g. Scan, Filter, Sort).
_Avoid_: node, executor, step

## Interface

**REPL**:
The interactive prompt where a user types SQL statements and sees results.
_Avoid_: shell, console, CLI

**Meta Command**:
A backslash-prefixed REPL instruction that is not SQL, such as `\dt` or `\d users`.
_Avoid_: dot command, slash command
