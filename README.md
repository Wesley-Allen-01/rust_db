# rust_db

`rust_db` is a deliberately tiny persistent key-value database. It is meant to
be read by someone building a database for the first time, not used in
production.

It has five operations:

- `set(key, value)` creates or replaces a value.
- `get(key)` reads a value.
- `remove(key)` deletes a value.
- `keys()` lists the current keys.
- `open(path)` creates a database or reopens one from disk.

## Try it

```console
$ cargo run -- notes.db
Opened notes.db
Commands: set KEY VALUE, get KEY, remove KEY, keys, exit
> set greeting hello database
OK
> get greeting
hello database
> keys
greeting
> exit
```

Run it again with the same file and the value will still be there.

## How it works

The whole database is a `HashMap<String, String>` in memory. Each change is
also appended to a plain-text log:

```text
SET\tgreeting\thello database
REMOVE\tgreeting
```

When the database opens, it reads those records from top to bottom and repeats
each operation to rebuild the map. This is called **log replay**. Appending is
simple, and reading from the in-memory map is fast.

The implementation is in [`src/lib.rs`](src/lib.rs), the small interactive
program is in [`src/main.rs`](src/main.rs), and the behavior tests are in
[`tests/database.rs`](tests/database.rs).

## Intentional limitations

Keeping the design small makes its tradeoffs easy to see:

- Keys and values cannot contain tabs or newlines because those separate fields
  in the log.
- The log only grows; there is no compaction.
- The entire database must fit in memory.
- Only one process should open a database file at a time.
- There are no transactions, indexes, query language, or crash recovery.

Those limitations are natural starting points if you want to grow the project.
