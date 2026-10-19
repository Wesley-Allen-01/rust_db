rust_db is a SQL database built for learning depth: a hand-written parser, executor and storage engine behind a psql-style REPL. Milestones: M1 in-memory SQL REPL → M2 saved to disk → M3 B-tree indexes → M4 WAL and transactions → M5 JOINs, aggregates, planner. Each milestone is a parent issue labelled `milestone-N` with its slices as sub-issues.

## Working style

- **Responses**: terse and plain. Lead with the result, use short sentences, and make every sentence carry information. The user wants to read them at a glance.
- **Code**: readable on one pass. Small functions, plain names, the simplest approach that works. Comments explain *why*. Match the surrounding code.

## Skills

Use the Matt Pocock skills (`mattpocock-skills:*`) for every stage of work:

- **Planning a milestone**: `grill-with-docs`
- **Implementing**: `tdd`, test-first, against the seams in `docs/agents/testing.md`. Use `codebase-design` when a module's interface is in question.
- **Before opening a PR**: `code-review` against `main`, then apply its findings. This is the refactor stage.
- **PR body**: `pr`
- **Red CI, a bug, or slowness**: `diagnosing-bugs`
- **Open design question**: `prototype`. **Facts needed**: `research`.
- **New domain terms or ADRs**: `domain-modeling`
- **Editing agent docs** (this file, `docs/agents/`): `writing-for-agents`

## Orchestration

When asked to work through issues or a milestone, follow `docs/agents/orchestration.md`. Parallelise wherever the dependency graph allows. Agents merge their own PRs once CI passes; there is no human review.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for Wesley-Allen-01/rust_db, managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default labels: needs-triage, needs-info, ready-for-agent, ready-for-human and wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
