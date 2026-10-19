# Execute Statements as a Volcano-style tree of Operators

Statements are turned into a Plan: a tree of Operators (Scan, Filter, Project, Sort, Limit, ...), each pulling Rows from its children one at a time. We picked this over walking the parsed statement directly (gets tangled as SQL features pile up) and over a SQLite-style bytecode VM (a fine design, but a harder first build). Its big advantage is that indexes and a cost-based planner later become "swap or reorder Operators" rather than a rewrite.
