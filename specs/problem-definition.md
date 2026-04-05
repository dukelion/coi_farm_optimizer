# CoI Rust Solver Problem Definition

## Purpose

This project solves a farm-planning problem for Captain of Industry.

Given:

- a set of enabled building counts
- a selected food basket
- optional extra monthly output requirements
- a globally selected fertilizer product
- a baseline fertility target per building type

the solver chooses crop rotations and downstream processing chains that maximize supported population while accounting for:

- crop yields
- fertility and fertilizer usage
- water usage
- recipe-chain conversion of crops into foods and extra outputs


## Inputs

The current scenario surface is defined by [`ScenarioConfig`](../src/scenario.rs):

- `building_counts: BTreeMap<BuildingType, u32>`
- `foods: Vec<String>`
- `food_multiplier: f64`
- `extra_requirements: BTreeMap<String, f64>`
- `fertilizer: Option<FertilizerProduct>`
- `baseline_fertility_by_building: BTreeMap<BuildingType, Option<f64>>`

### Building types

Supported building types:

- `FarmT1`
- `FarmT2`
- `FarmT3`
- `FarmT4`

Display labels currently map to:

- `Farm`
- `Irrigated Farm`
- `Greenhouse`
- `Greenhouse II`

### Foods

The settlement demand model currently recognizes:

- `Potatoes`
- `Corn`
- `Bread`
- `Meat`
- `Eggs`
- `Tofu`
- `Sausage`
- `Vegetables`
- `Fruit`
- `Snack`
- `Cake`

Food categories are:

- `Carbs`: `Potatoes`, `Corn`, `Bread`
- `Protein`: `Meat`, `Eggs`, `Tofu`, `Sausage`
- `Vitamins`: `Vegetables`, `Fruit`
- `Treats`: `Snack`, `Cake`

### Extra requirements

`extra_requirements` is a monthly amount map for outputs that must also be satisfied.

Examples:

- direct crop/material requirements like `Saplings`
- direct crop/material requirements like `Poppy`
- processed outputs like `Food Pack`
- processed outputs like `Cooking Oil`, `Sugar`, and `Ethanol`

Direct crop requirements reduce the crop supply available to food chains.
Recipe-backed extra requirements are satisfied by dedicated requirement variants.


## Source Data

### Crop data

Crop simulation data comes from:

- [`data/wiki_crop_data.json`](../data/wiki_crop_data.json)

The runtime loader is in:

- [`src/io/wiki.rs`](../src/io/wiki.rs)

Important rules:

- crops are loaded from wiki JSON
- missing wiki-backed crop data means the crop is ignored
- `Fruit` and `Green Manure` still load from JSON, but through custom loader interpretation rather than hardcoded fallback values

### Recipe data

Authoritative processing recipes come from:

- [`captain-of-data/data/machines_and_buildings.json`](../captain-of-data/data/machines_and_buildings.json)

The recipe loader is in:

- [`src/io/recipe.rs`](../src/io/recipe.rs)

The solver currently uses:

- flattened food variants
- flattened requirement variants
- flattened slack sink variants

Chains are:

- normalized to exposed materials
- flattened across relevant upstream recipe steps
- deduplicated when identical after flattening

There is no dominance pruning yet.


## Crop and Fertility Model

The fertility model lives in:

- [`src/domain/fertility.rs`](../src/domain/fertility.rs)

### Rotation summary

For a candidate rotation, the simulator computes:

- average fertility drain / replenishment
- fertility equilibrium
- fertilizer required per month at the chosen target
- water per month
- effective crop yields per month

The simulator works from wiki-backed per-tier crop properties rather than earlier guessed prototype constants.

### Baseline fertility

Each building type receives exactly one baseline fertility target during phase 1:

- `None` means natural / no forced target
- a numeric value means all options for that building in phase 1 are simulated at that target

The phase-1 catalog builder enforces this:

- [`build_baseline_catalog_by_building`](../src/domain/catalog.rs)

### Fertilizer product

The selected fertilizer product is global to the scenario:

- `None`
- `Organic Fert`
- `Fertilizer I`
- `Fertilizer II`

It controls:

- whether fertilization is possible
- target caps
- fertilizer quantity conversion for supported targets


## Settlement Demand Model

The settlement demand model is in:

- [`src/domain/settlement.rs`](../src/domain/settlement.rs)

### Demand semantics

Demand is defined per 100 population and split by:

- number of fulfilled food categories
- number of selected foods within each category

For a selected food:

- `demand_per_100_for_food(food)` returns the exact monthly requirement per 100 population
- actual scenario demand is scaled by `food_multiplier`

This exact per-food demand is now the canonical demand used by both:

- the allocation solver
- the report bottleneck logic


## Optimization Overview

At a problem-definition level, the current app has two logical steps:

1. choose farm rotations for the available buildings
2. allocate the resulting crop basket into foods and extra outputs

Implementation details for those steps live in:

- [solver.md](solver.md)


## Outputs

At the user/problem level, the solver produces:

- supported population
- selected rotations
- crop output
- food output
- extra output
- fertilizer usage
- water usage

The detailed Rust result structures and solver-stage outputs are documented in:

- [solver.md](solver.md)


## Report Semantics

The app reports the final solved scenario in terms of:

- selected rotations
- final outputs
- bottlenecks
- total fertilizer and water

Detailed report semantics that depend on implementation choices live in:

- [solver.md](solver.md)
- [gui.md](gui.md)


## Current Known Behavior

Current implementation-specific behavior is documented in:

- [solver.md](solver.md)


## Non-Goals for the Current Version

Solver-specific non-goals and diagnostics are documented in:

- [solver.md](solver.md)


## Summary

This document describes the domain problem the app is trying to solve:

1. plan crop production for a constrained set of farms
2. satisfy a selected basket of settlement foods
3. satisfy optional extra monthly outputs
4. account for fertility, fertilizer, water, and processing chains

Implementation details for the current solver architecture are intentionally separated into:

- [solver.md](solver.md)
