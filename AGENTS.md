# AGENTS.md

## Scope

This file applies to:

- `.`

Use this file for repo-level guidance only.
Detailed behavior and model semantics belong under:

- `specs/`


## Project Overview

`coi-rust` is the Rust rewrite of the Captain of Industry farming and food optimization tool.

At a high level, the app:

- loads crop and recipe source data
- builds candidate farm rotations
- solves a farm-selection optimization
- allocates crop output into foods and extra outputs
- exposes the result through a Slint desktop UI and a CLI


## Repo Layout

- `src/domain`
  - core solver and domain logic
- `src/io`
  - source-data loading
- `src/scenario.rs`
  - top-level scenario input and runner
- `src/report.rs`
  - report formatting
- `src/main.rs`
  - desktop UI entry point
- `src/bin`
  - CLI and diagnostics
- `ui`
  - Slint UI files
- `data`
  - repo-owned input snapshots
- `captain-of-data`
  - authoritative recipe-data submodule
- `specs`
  - detailed project specs


## Source of Truth

When behavior is ambiguous, prefer the spec files over this guide.

Current spec entry point:

- `specs/problem-definition.md`


## Contributor Expectations

### Keep detailed rules in `specs/`

Do not expand `AGENTS.md` with detailed solver rules, UI interaction rules, or model semantics.
Put those in one or more files under `specs/`, then link them from here if needed.

### Prefer tests first

This repo has been developed incrementally with test-first changes.
Keep that pattern, especially for:

- solver math
- demand/allocation behavior
- recipe-chain logic
- report semantics
- UI value-mapping logic

### Keep related logic aligned

This project is sensitive to near-duplicate logic drifting apart.
When changing behavior, check for matching logic in the corresponding layers, especially:

- solver vs report
- UI editor state vs saved scenario format
- source-data loader vs domain assumptions

### Make small, explicit changes

Prefer fixing the narrowest layer that owns the bug.
If behavior is changing intentionally, update the relevant spec file in the same change.


## Validation

At minimum, run:

```powershell
cargo test
```

When useful, also run:

```powershell
cargo run --bin coi-cli -- last_scenario.json
```

and, for allocation-stage debugging:

```powershell
cargo run --bin compare_stages -- last_scenario.json
```


## Spec Maintenance

If a change affects:

- problem definition
- solver objectives
- allocation semantics
- report meaning
- UI interaction contract

then update or add the appropriate file under `specs/`.
