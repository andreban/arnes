# Contributing

Thanks for your interest in arnes. The project is in early-stage
development; the design is captured in
[`docs/implementation-plan/`](docs/implementation-plan/index.html) and
[`docs/build-your-own-harness/`](docs/build-your-own-harness/index.html).
Read those first &mdash; most "should we do X?" questions are already
answered there.

## Filing issues

Open an issue on the GitHub repository. Useful issues include:

- A reproducer (commit SHA, command, expected vs. actual output) for bugs.
- A link to the milestone or PR slug
  (`docs/implementation-plan/m1-plan.html#pr4`) for design questions.

## Pull request process

1. Read [`AGENTS.md`](AGENTS.md) &mdash; it lists the rules CI enforces.
2. Branch from `main` with a conventional-commits-style name and short
   slug: `feat/<slug>`, `fix/<slug>`, `docs/<slug>`, `chore/<slug>`. No
   plan-number prefix &mdash; the PR description references the relevant
   work-plan section instead. Keep PRs scoped to one unit of work from
   the current milestone's plan (e.g. `m1-plan.html#pr3`); if your
   change spans multiple, split it.
3. Run the full local check matrix:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

4. Open the PR against `main`. Title in the imperative; description should
   say what changed and why, and link the relevant milestone-plan section.
5. CI must be green before merge. Each PR is expected to leave `main` in a
   working state &mdash; no "fixes incoming next PR" merges.

There is no DCO sign-off requirement at this stage.

## Running the test matrix locally

Same three commands as the PR checklist above. CI additionally runs them
on Linux, macOS, and Windows; if you only have one OS available, that is
fine &mdash; flag any platform-specific changes in the PR description so
reviewers can pay extra attention to the CI results.
