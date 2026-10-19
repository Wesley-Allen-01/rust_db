# Hand-write the database core; use crates only for peripherals

rust_db exists for learning depth, not to compete as a product. So the SQL parser, executor, and storage engine are written from scratch even where mature crates exist (e.g. `sqlparser`, `sled`). Crates are fine for concerns that teach nothing about databases: terminal line editing, CLI argument parsing, checksums. Don't replace a core layer with a crate to save time; that defeats the purpose.
