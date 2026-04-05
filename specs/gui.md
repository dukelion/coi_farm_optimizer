# GUI Architecture

## Overview

The GUI is a Slint desktop application defined in `ui/appwindow.slint` and wired to application logic in `src/main.rs`.

The architecture is intentionally split into a small entry-point layer plus focused UI helper modules:

1. `ui/appwindow.slint`
   Declares the window, visual layout, view state, and user-triggered callbacks.
2. `src/main.rs`
   Owns callback registration, threading, scenario load/save/reset orchestration, and solver execution/progress updates.
3. `src/ui_support/scenario_binding.rs`
   Maps between Slint properties and `ScenarioConfig`.
4. `src/ui_support/baseline_editor.rs`
   Owns baseline slider/badge conversion helpers.
5. `src/ui_support/extra_targets.rs`
   Owns structured advanced-target row management and parsing.

The solver and scenario model remain outside the UI layer. The GUI ultimately translates user input into `ScenarioConfig`, runs the solver, and renders the resulting report text.

## Main Screen Layout

The app window is a single `AppWindow` component with a three-part vertical structure:

1. Top file/menu row
   Contains `Load`, `Save`, and `Reset`, plus a short instructional label.
2. Main content area
   A two-pane horizontal layout:
   - Left: `Inputs`
   - Right: `Report`
3. Bottom action row
   Contains the main solve button, which toggles between `Run Solve` and `Stop Solve`, plus the current status text.

### Inputs Pane

The left pane is a scrollable form organized into four grouped sections:

- `Buildings`
  Four farm rows (`Farm T1` through `Farm T4`) with:
  - building count `SpinBox`
  - baseline fertility indicator button
- `Foods`
  Checkboxes grouped into:
  - Carbs
  - Protein
  - Vitamins
  - Treats
- `Solver Settings`
  - fertilizer `ComboBox`
  - food multiplier `LineEdit`
- `Advanced`
  Structured editor for extra solver targets:
  - target picker `ComboBox`
  - `+` button to add a target row
  - up to four visible target rows with value editors and remove buttons

### Report Pane

The right pane is a read-only `TextEdit` used as the report/output surface. It displays:

- solver output
- validation errors
- runtime status results

## State Model in Slint

`AppWindow` stores UI state as `in-out property` values. The main categories are:

- Scenario inputs
  - farm counts
  - selected foods
  - fertilizer selection
  - baseline fertility labels
  - food multiplier text
- Advanced target editor
  - selected target in dropdown
  - four row visibility flags
  - four row names
  - four row values
  - a synced JSON string representation in `extra-requirements-json`
- Solver/report state
  - `status-text`
  - `report-text`
  - `solving`
- Baseline popover state
  - open/closed flag
  - active building name
  - slider bounds
  - slider value
  - display badge text
  - popover anchor position

This is a view-model-style setup: Slint owns the immediate visual state while Rust mutates it through generated setters/getters.

## Callback Architecture

The Slint component exposes callbacks, and Rust registers handlers for them during startup.

Core callbacks:

- `run-solver`
- `load-scenario`
- `save-scenario`
- `reset-scenario`
- `open-baseline-editor`
- `cancel-baseline-editor`
- `apply-baseline-editor`
- `preview-baseline-editor-value`
- `add-extra-target`
- `remove-extra-target`

This keeps layout declarative in Slint while behavior stays in Rust.

## Baseline Fertility Popover

Baseline editing is implemented as a lightweight anchored popover rather than a modal dialog.

Behavior:

- Clicking a baseline button opens the popover near the clicked row.
- Clicking outside applies the current selection.
- There are no visible `Apply` or `Cancel` buttons in the current popover.
- The slider is the only visible input control for the value.
- Slider semantics:
  - leftmost position = `Off`
  - non-zero values map to real fertility baselines starting at `60%`
- The badge updates live while dragging.
- Applying `Off` is stored back into the scenario as `Natural`.

Rust helper functions in `src/ui_support/baseline_editor.rs` handle slider/baseline conversion:

- `baseline_editor_range_for_fertilizer`
- `quantize_slider_value`
- `slider_value_to_baseline`
- `baseline_to_slider_value`

## Advanced Targets

The `Advanced` section no longer edits raw JSON directly. Instead it exposes a structured row editor.

Current supported targets:

- `Saplings`
- `Cooking Oil`
- `Sugar`
- `Ethanol`
- `Poppy`
- `Food Pack`

Implementation details:

- The UI supports up to four visible rows.
- Adding a target inserts a row if it is not already present.
- Removing a row compacts the remaining rows upward.
- Each row stores a string value in the UI and is parsed into `f64` in Rust.
- Rust keeps `extra-requirements-json` synchronized as a compatibility/debug representation.

Main helper functions in `src/ui_support/extra_targets.rs`:

- `set_extra_target_rows`
- `clear_extra_target_rows`
- `set_extra_target_slot`
- `add_or_update_extra_target`
- `remove_extra_target_at`
- `extra_target_entries`
- `extra_requirements_from_ui`
- `sync_extra_requirements_json`

## Scenario Mapping

The UI mapping layer performs two important translations:

1. `apply_scenario_to_ui`
   Maps `ScenarioConfig` into Slint properties when loading/resetting.
2. `scenario_from_ui`
   Collects Slint property state and produces a validated `ScenarioConfig`.

This is the primary boundary between the GUI and the solver/scenario domain.

## Solver Execution Flow

`run-solver` triggers the following flow:

1. Collect UI state into `ScenarioConfig`
2. Persist the last scenario to `last_scenario.json`
3. Mark UI as solving
4. Spawn a worker thread for phase-1 scenario solving
5. Stream progress back through `slint::invoke_from_event_loop`
6. Support cancellation through an `Arc<AtomicBool>`
7. Render the final formatted report into the report pane

Progress/status responsibilities:

- foreground UI thread updates `status-text`
- background solver thread computes results
- completion path writes final report text and resets `solving`
- while solving, the main action button switches from `Run Solve` to `Stop Solve`

## Persistence

The GUI uses `last_scenario.json` as the main persistence target for quick restore.

Supported actions:

- load last scenario on app startup if present
- explicit load
- explicit save
- reset to hardcoded defaults in `default_scenario_config`

## Current Design Constraints

Important current constraints in the GUI architecture:

- advanced targets are limited to four visible rows
- advanced target options are currently hardcoded
- report rendering is plain text, not structured rich UI
- popover placement is manually anchored from clicked button coordinates
- many UI fields still use string properties rather than typed domain models

These are acceptable for the current app size, but they are the main pressure points if the GUI grows more dynamic later.
