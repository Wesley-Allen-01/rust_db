# Orchestration

How an orchestrating agent drives a milestone to completion. `gh` commands for sub-issues and blocking live in `issue-tracker.md`.

## Guardrails

- `main` is protected: PRs only, the `check` CI job (fmt, `clippy -D warnings`, tests) must pass against the latest `main`, squash merges only, and the rules apply to admins. Every change, docs included, goes through a PR.
- When CI goes red, fix the code on the branch. Changing CI or lint settings needs the user's approval.
- The user doesn't review PRs. CI passing is the merge gate.

## Loop

1. **Find the frontier.** The current milestone is the open parent issue with the lowest `milestone-N` label. Its frontier is every open `ready-for-agent` sub-issue with no open blockers and no assignee.
2. **Dispatch in parallel.** Start one worker subagent per frontier issue, all at once, each in its own git worktree (`isolation: "worktree"`). Assign each issue to `@me` first. A worker:
   1. Reads the issue, `GLOSSARY.md` and the ADRs it touches.
   2. Builds the slice test-first: unit tests plus `.slt` files for SQL behaviour.
   3. Gets `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` passing locally.
   4. Opens a PR whose body says `Closes #N`, then runs `gh pr merge --squash --auto`.
   5. Finishes when the PR is merged. If CI fails, it fixes and pushes. If `main` has moved, it rebases onto `main` (or runs `gh pr update-branch`) and resolves conflicts.
3. **Repeat.** Each time a PR merges, recompute the frontier and dispatch any newly unblocked issues right away. Don't wait for the whole batch to finish.
4. **Milestone done** when every sub-issue of the parent is closed. Close the parent, then stop and suggest the user start a `/grill-with-docs` session for the next milestone. Each milestone is designed only when it's reached.

## Maximising parallelism

- When filing new slices, cut them so they block each other as little as possible: separate modules, separate files, thin interfaces agreed up front in the issue body.
- If two frontier issues are likely to edit the same lines, still run them in parallel, and let the second to merge rebase.
- If a worker finds that an issue is underspecified, it comments the question on the issue, relabels it `needs-info`, and moves on. Other frontier work continues.
