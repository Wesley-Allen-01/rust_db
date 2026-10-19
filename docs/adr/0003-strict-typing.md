# Strict, Postgres-style typing

Every Column has a fixed Data Type, and Values that don't match are rejected, apart from one Coercion: INT to FLOAT. Comparing incompatible types (`WHERE name = 3`) is a type error, not just false. We picked this over SQLite's dynamic typing because it matches the existing Schema design and it makes type checking a real stage of query processing. Don't loosen it to "be compatible with SQLite".
