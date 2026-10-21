# Branch lifecycle policy — Akula / NOQ / Roha (2026-10-21)

## Goal
Do not accumulate hundreds of test branches, but NEVER mistake a green unit test for production acceptance or delete active stacked experiments.

## GitHub admin settings (manual; connected app has no Administration permission)
Settings → General → Pull Requests:
- Enable **Allow auto-merge**.
- Enable **Automatically delete head branches**.
- Prefer **Allow squash merging** as the normal integration method (keep merge/rebase for explicit upstream provenance when necessary).
Settings → Rules → Rulesets, or Branch protection for `main`:
- Require pull requests and minimum required **reliable, always-running** CI checks.
- Require branch up to date or merge queue only after validating runner capacity; avoid making unavailable self-hosted jobs hard gates.
- Block force pushes and branch deletion of `main`; disallow bypass by normal agents.
- Protect releases and long-lived refs as appropriate.
- Auto-merge should be activated **per ready PR**, not for Draft PRs or for a whole old experimental stack.
- Be aware that GitHub's native delete-on-merge removes the source branch even if other PRs use it as base. For stacked PRs, first retarget dependents to main or a retained integration branch; only then merge or switch on automatic deletion. Disable native automatic delete temporarily if stacked PRs cannot be safely retargeted.

## Workflow included here
- `.github/workflows/branch-hygiene.yml`: on a **merged** PR into repository default branch, removes only the exact unmoved non-protected in-repository head ref, after checking there are no open dependent/head PRs. Git ref deletion uses an exact SHA lease. If native auto-delete has already removed it, no-op.
- Weekly/manual read-only inventory: flags branches whose tip commits are at least 14 days old with no open PR using them as head or base; **never automatically deletes unmerged heads**.
- The existing `cleanup.yaml` cleans docs-preview content, not Git refs, so do not remove or repurpose it.
- Experiment/Draft branches are kept until merged or explicitly disposed of after evidence is retained.
- Retarget stacked PRs rather than silently closing or deleting them.

## Promotion gates
Research → reproducer RED → targeted GREEN → full CI on exact head SHA → correct base / dependencies → ready-for-review → auto-merge eligible → merged to main → integrated product test.
For multi-platform network changes require Linux, Windows, cross-platform compile and pinned network/fault test relevant to changed code.
A temporary failure due to lack of self-hosted runner is not a pass; do not bypass the required gate.

## Manual triage of older branches
1. List open PRs and their head/base refs; protect both.
2. Classify every orphan tip: merged ancestor, duplicate, actively used test, obsolete, or unknown.
3. Preserve evidence (PR/commit SHA/test and migration result).
4. Only delete unmerged orphan branches through separate explicit human-reviewed batch. No age-only deletion.

## Deployment
This policy and workflow are submitted by PR. They do not change GitHub Settings. The branch-hygiene workflow starts after the PR is merged to main. Admin configuration must be done by a repository administrator.
