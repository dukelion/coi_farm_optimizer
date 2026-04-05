# Solver Architecture

## Purpose

This document describes how `coi-rust` currently solves the optimization problem.

For the domain-level problem statement, inputs, and source data model, see:

- [problem-definition.md](problem-definition.md)


## Top-Level Flow

The main scenario entry points are:

- [run_phase1_scenario](../src/scenario.rs)
- [run_phase1_scenario_embedded](../src/scenario.rs)
- [run_phase1_scenario_embedded_with_progress](../src/scenario.rs)

Current flow:

1. load crop catalog
2. load authoritative recipe data
3. derive crops needed for foods and requested extra requirements
4. build baseline-only option catalogs per building
5. solve the farm-selection MIP
6. evaluate the chosen crop basket through the allocation solver
7. render/report the final stabilized result

Shared scenario preparation for both main entry points is factored through:

- `src/scenario_prep.rs`


## Phase 1: Farm Selection

Farm selection is implemented in:

- [src/domain/optimizer.rs](../src/domain/optimizer.rs)

Current main path:

- `optimize_building_mix_mip(...)`

### Model shape

The phase-1 model is count-based and uses:

- integer variables for option counts per building pool
- continuous variables for downstream process-chain runs
- a shared continuous `population` variable

The model works over already-generated catalog options, not raw crop-slot binaries.

### Objective

Current scalarized objective:

- maximize `population * 1_000_000 - cost`

where:

- `cost = 2 * fertilizer_per_month + water_per_month`

This is intended to encode the priority:

1. maximize population
2. among equal-pop solutions, prefer lower fertilizer
3. then lower water

This is not strict lexicographic optimization.

### Constraints

The model enforces:

- exact farm-count usage per building pool
- crop production limits from selected farm options
- food-demand satisfaction
- recipe-backed extra requirements
- direct crop/material requirements


## Allocation Solver

Allocation is implemented in:

- [src/domain/allocation.rs](../src/domain/allocation.rs)

Main entry point:

- `evaluate_population_from_crop_outputs(...)`

This takes the crop basket produced by phase 1 and solves which process chains to run.

### Demand rates

Allocation uses exact per-food demand based on:

- `SettlementFoodConsumption::demand_per_100_for_food(...)`

with conversion to per-person monthly rate:

- `demand_per_100 / 100`

Important rule:

- do not use rounded display demand in the allocation solver
- phase 1 now also builds food-demand constraints from the same exact per-food demand semantics

This keeps phase 1, allocation, and reporting aligned on demand math.


## Two-Stage Allocation

The current allocation logic is intentionally two-stage.

### Allocation stage 1

Stage 1 solves:

- maximize supported population

Subject to:

- crop availability
- selected food demand
- extra recipe-backed requirements

### Allocation stage 2

Stage 2 re-solves on the same crop basket with:

- population fixed to a stabilized target
- objective changed to maximize slack outputs

The stabilized target is:

- rounded down to the nearest `100` if population is at least `100`
- otherwise floored to an integer

Implemented by:

- `round_down_population_target(...)`

This means stage 2 may use a different process-chain mix than stage 1.

That is expected behavior.


## Slack Output Policy

When stage 2 has spare crop capacity after satisfying the stabilized population target, it prefers surplus conversion into exposed byproducts.

Only requested recipe-backed requirement variants are activated. Unrequested requirement chains are not available as generic slack outputs.

Current priority:

1. `Compost`
2. `Animal Feed`

Slack sink variants are built from:

- [src/io/recipe.rs](../src/io/recipe.rs)

Shared allocation-problem preparation is factored through:

- `src/domain/allocation_problem.rs`


## Progress and Cancellation

Progress and interruption are implemented in:

- [src/domain/optimizer.rs](../src/domain/optimizer.rs)

Current solver backend:

- HiGHS through `good_lp` plus direct callback wiring

Current callback behavior:

- periodic logging updates
- improving-solution updates
- interrupt support

UI-facing progress currently reports:

- elapsed time
- incumbent objective
- estimated population
- dual bound
- MIP gap
- node count

Cancellation:

- phase 1 can be interrupted
- if there is a feasible incumbent, downstream reporting still completes from that incumbent


## Outputs

Main result type:

- [OptimizationResult](../src/domain/optimizer.rs)

It includes:

- `phase1_interrupted`
- `supported_population`
- `total_fertilizer_per_month`
- `total_water_per_month`
- `crop_outputs`
- final allocation result
- selected options
- selected options by building

Allocation result includes:

- supported population
- food outputs
- extra outputs
- process runs
- crop inputs used
- crop inputs remaining


## Report Semantics That Depend on Solver Behavior

The report formatter is in:

- [src/report.rs](../src/report.rs)

Solver-related report rules:

- `Supported Population` is the final stabilized allocation population
- `Recipe Chains Used` are the final stage-2 process chains
- `Bottleneck Food` must use the same demand semantics as the solver
- `Bottleneck Crop` is currently the crop with the highest used/available ratio


## Diagnostics

Useful debugging paths:

- `cargo test`
- `cargo run --bin coi-cli -- last_scenario.json`
- `cargo run --bin compare_stages -- last_scenario.json`

Diagnostic helper for direct allocation-stage comparison:

- `compare_allocation_stages_from_crop_outputs(...)`


## Current Known Behavior

### Stage-2 chain changes are normal

Because stage 2 changes objective and fixes a stabilized population, it may:

- switch between pack chains
- shift between eggs/meat/tofu routes
- consume different crops while still meeting the fixed target
- create additional slack outputs

### Final population may be lower than exact stage-1 max

This is intentional because of stabilization.

Example:

- stage 1 may find `981.736`
- stage 2 may then run at `900`

### No farm-option pruning yet

There is currently:

- flattening and dedupe of recipe variants
- no dominance pruning of farm options
- no dominance pruning of recipe variants beyond exact dedupe


## Non-Goals of the Current Solver

The current solver does not yet guarantee:

- strict lexicographic optimization
- preservation of stage-1 chain mix into stage 2
- dominance-pruned farm catalogs
- complete parity with any earlier prototype search/report behavior
