# `coi-rust`

`coi-rust` is the Rust implementation of the Captain of Industry farming and food optimization tool.

It currently:

- loads crop data from [`data/wiki_crop_data.json`](data/wiki_crop_data.json)
- loads recipe data from the bundled [`captain-of-data`](captain-of-data) submodule
- builds farm rotation candidates by building tier
- solves a farm-selection optimization
- allocates resulting crop output into foods and optional extra outputs
- exposes the workflow through both a Slint desktop UI and CLI binaries

## Setup

Clone with submodules, or initialize them after cloning:

```powershell
git clone --recurse-submodules <repo-url>
```

or:

```powershell
git submodule update --init --recursive
```

Notes:

- recipe data is loaded from the `captain-of-data` submodule
- `data/wiki_crop_data.json` is currently a repo-owned snapshot used by the crop loader

## Repo layout

- `src/domain` - solver and domain logic
- `src/io` - source-data loading
- `src/scenario.rs` - scenario loading and top-level runner
- `src/report.rs` - report formatting
- `src/main.rs` - Slint desktop entry point
- `src/ui_support` - UI state mapping and editor helpers
- `src/scenario_prep.rs` - shared scenario preparation for solver entry points
- `src/bin/coi-cli.rs` - CLI scenario runner
- `src/bin/compare_stages.rs` - allocation-stage comparison helper
- `ui` - Slint UI files
- `data` - repo-owned crop input snapshots
- `specs` - project behavior and architecture docs

## Running

Desktop UI:

```powershell
cargo run --bin coi_farm_optimizer
```

CLI report for a scenario file:

```powershell
cargo run --bin coi-cli -- scenario.json
```

Allocation-stage comparison:

```powershell
cargo run --bin compare_stages -- last_scenario.json
```

## Testing

```powershell
cargo test
```

## Releases

GitHub Actions builds a Windows release binary for `coi_farm_optimizer`.

- every PR and push to `master` uploads a downloadable CI artifact
- pushing a tag like `v0.1.0` also attaches `coi_farm_optimizer-windows-x86_64.zip` to the corresponding GitHub Release

## Contributing

Contributions made with LLM assistance are welcome.

- `AGENTS.md` contains repo-specific guidance intended to help agent-assisted and human contributors stay aligned
- detailed behavior and architectural rules live under [`specs/`](specs/)

## License

Licensed under either of:

- Apache License, Version 2.0, see [`LICENSE-APACHE`](LICENSE-APACHE)
- MIT license, see [`LICENSE-MIT`](LICENSE-MIT)

at your option.

## Specs

Start with:

- [`specs/problem-definition.md`](specs/problem-definition.md)
- [`specs/solver.md`](specs/solver.md)
- [`specs/gui.md`](specs/gui.md)
