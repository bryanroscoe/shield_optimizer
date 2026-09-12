# Where everything is

Master index, written 2026-09-12 as Gas Town was wound down. Gas Town was a
multi-agent orchestration workspace at `~/Developer/gastown` coordinating four
projects. It is going away. Each project now runs as a standalone agent from its
own folder in `~/Developer`. This file lists every location so nothing is lost.

## The four projects

| Project | Standalone folder | GitHub | Backlog |
|---|---|---|---|
| Accrete, incremental game in Godot | `~/Developer/accrete` | `bryanroscoe/accrete` | 52 issues labeled `gastown` |
| Novalyst, book compendium and reader | `~/Developer/Novalyst` | `bryanroscoe/Novalyst` | 41 issues labeled `gastown` |
| Shield Optimizer, Android TV app | `~/Developer/shield_optimizer` | `bryanroscoe/shield_optimizer` | 22 issues labeled `gastown` |
| Saget, home server operations | `~/Developer/sagetserver` | `bryanroscoe/sagetserver` | 43 issues labeled `gastown` |
| Gas Town HQ, tooling and coordination | `~/Developer/gastown` | `bryanroscoe/gastown-hq` (private) | 28 issues labeled `gastown` |

Every issue is titled `[bead-id] title`, so references to old bead ids such as
`ac-mlg.4` or `nv-sus` still resolve by searching the issue list. Each repository
also holds, under `docs/`:

- `GASTOWN-HANDOFF.md`, orienting a fresh agent with no Gas Town context.
- `gastown-*-beads-export.json`, the complete tracker export including closed
  items, with full descriptions, acceptance criteria and comment history. This is
  the authoritative record where an issue body was trimmed for length.

## What is in git and what is not

In git and pushed: all source, the design and engineering record, acceptance
evidence, decision packets, run reports and the bead exports. The preservation
sweep on 2026-09-12 moved several bodies of work that had been living only in
agent workspaces into the repositories:

- Accrete: 111 design files, the canonical gameplay design, tuning decisions and
  every playtest judgment including Bryan's verbatim reviews, now `docs/design/`.
- Novalyst: 87 crew and polecat documents, the entire review and acceptance record.
- Saget: 101 branch-only candidate files under `candidates/`, twelve task
  directories with their scripts, tests and fixtures, all re-tested from the new
  paths, plus the T61 relinker which had been sitting in no git repository at all.
- Shield: the release decision packet, `docs/RELEASE-DECISION-2026-09-09.md`,
  with the commit plan, draft changelog and the physical device test script.
- Gas Town HQ: 410 Mayor briefs, reviews and decision packets under
  `mayor/artifacts/`.

Not in git, on disk only, too large or not source:

| Path | Size | What it is |
|---|---|---|
| `~/Developer/accrete/builds/accrete-recovery-r08-20260911f` | 422 MB | The current playable Accrete build, Mac app plus Windows zip, copied out of the Gas Town tree. Launch guide is `README-AMENDED-HANDOFF.md` inside it. |
| `~/Developer/gastown/accrete/.runtime` | 9.2 GB | Every Accrete build and QA run, R01 through R08F, with receipts and captures. Superseded builds; keep only if you want the full history. |
| `~/Developer/gastown/mayor/artifacts` | 4.4 GB | Mayor working tree. The documents from it are in `bryanroscoe/gastown-hq`; the rest is build output, caches and vendored upstream checkouts. |
| `~/Developer/gastown/.dolt-data` | 196 MB | The original beads databases. The JSON exports carry the same content in readable form. |
| `~/Developer/Novalyst/test_books` | 13 GB | Novalyst book sources and generated output, including the owned Neuromancer EPUB. |
| `~/Developer/recovery-archives` | 31 GB | Pre-existing archive, unrelated to the shutdown. |
| `~/Developer/gastown-quarantine` | 12 KB | A stale beads directory moved aside on 2026-09-09 because the scheduler was scanning it every 30 seconds. Safe to delete. |

## Other checkouts that exist

- `~/Developer/Novalyst-portraits-v2`, branch `codex/portraits-v2`, same remote as
  Novalyst, with uncommitted work. Flagged to the Novalyst owner during the sweep.
- `~/Developer/tmux-adapter`, branch `fix/web-adapter-history-copy-input`, remote
  `gastownhall/tmux-adapter`. Holds a built and deployed fix for terminal scroll
  history, text selection and Command-Backspace. It could not be pushed: the
  account has read access only on that repository. Commit `b438ef4` is local.
- `~/Developer/sagetbot`, `~/Developer/saget-org`, `~/Developer/tvintelligentsia`
  and others predate Gas Town and were not part of it. Several have uncommitted
  changes of their own.

## Project state at shutdown

**Accrete.** R08F is the current build and the first the crew cleared for play.
The measured performance fix took 200 loose pieces from 0.24x to 0.89x achieved
speed. Open: remaining copy and geometry polish, Bastion events never crew-played,
native relaunch unverified because the host screen stays locked, and deferred
save-persistence coverage.

**Novalyst.** Backend and Flutter client with green CI. A complete run over
*Neuromancer* produced 216 entities, 843 page snapshots and 24 recaps, reviewed
and accepted. Open: pricing, legal scope, entity density and recap length.

**Shield Optimizer.** All accepted work is on `main` in commits `36b561c` and
`d02de57`. Remaining: cut a release tag and run the physical device test.

**Saget.** Ten local candidates accepted, seven production changes applied with
rollback and independent verification. Remaining decisions are listed in
`sagetserver/crew/steward/review-evidence/bryan-decisions-20260908.md`, also in
the repository.

## Migration tool

`mayor/bd2gh.py` in `bryanroscoe/gastown-hq` converted beads to GitHub issues. It
is idempotent: re-running it creates only what is missing.
