#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use coi_rust::domain::crop::BuildingType;
use coi_rust::domain::fertility::FERTILIZER_ORGANIC;
use coi_rust::domain::optimizer::SolverProgress;
use coi_rust::report::format_phase1_report;
use coi_rust::scenario::{
    load_scenario_config, run_phase1_scenario_embedded_with_progress, save_scenario_config,
    ScenarioConfig,
};

mod ui_support;

use ui_support::baseline_editor::{
    baseline_editor_range_for_fertilizer, baseline_to_slider_value, current_baseline_label,
    quantize_slider_value, set_building_baseline_label, slider_value_to_baseline,
};
use ui_support::extra_targets::{
    add_or_update_extra_target, remove_extra_target_at, sync_extra_requirements_json,
};
use ui_support::scenario_binding::{apply_scenario_to_ui, scenario_from_ui};

slint::include_modules!();

fn main() {
    run_ui();
}

fn run_ui() {
    let app = AppWindow::new().expect("UI should initialize");
    let last_scenario_path = PathBuf::from("last_scenario.json");
    if let Ok(config) = load_scenario_config(&last_scenario_path) {
        apply_scenario_to_ui(&app, &config);
        app.set_status_text("Loaded last scenario".into());
    }
    let active_cancel = Arc::new(Mutex::new(None::<Arc<AtomicBool>>));
    let weak = app.as_weak();
    app.on_open_baseline_editor(move |building, x, y, height| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let building = building.to_string();
        let Some(current) = current_baseline_label(&app, &building) else {
            return;
        };
        let (min, max) = baseline_editor_range_for_fertilizer(&app.get_fertilizer_selection().to_string());
        let natural = current == "Natural";
        let value = if natural {
            0.0
        } else {
            baseline_to_slider_value(
                current
                .parse::<f32>()
                .map(|value| value.clamp(min as f32, max as f32))
                .unwrap_or(0.0),
                min,
                max,
            )
        };
        let baseline = slider_value_to_baseline(value, min, max);
        app.set_baseline_editor_building(building.clone().into());
        app.set_baseline_editor_min(min);
        app.set_baseline_editor_max(max);
        app.set_baseline_editor_value(value);
        app.set_baseline_editor_natural(natural);
        app.set_baseline_editor_x(x + 8.0);
        app.set_baseline_editor_y(y + height + 6.0);
        app.set_baseline_editor_display(
            if natural {
                "Off".into()
            } else {
                format!("{:.0}%", baseline).into()
            }
        );
        app.set_baseline_editor_open(true);
    });
    let weak = app.as_weak();
    app.on_preview_baseline_editor_value(move |value| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let snapped = quantize_slider_value(
            value,
            app.get_baseline_editor_min(),
            app.get_baseline_editor_max(),
        );
        if (app.get_baseline_editor_value() - snapped).abs() > f32::EPSILON {
            app.set_baseline_editor_value(snapped);
        }
        let baseline = slider_value_to_baseline(
            snapped,
            app.get_baseline_editor_min(),
            app.get_baseline_editor_max(),
        );
        app.set_baseline_editor_display(
            if baseline <= 0.0 {
                "Off".into()
            } else {
                format!("{baseline:.0}%").into()
            }
        );
    });
    let weak = app.as_weak();
    app.on_cancel_baseline_editor(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        app.set_baseline_editor_open(false);
    });
    let weak = app.as_weak();
    app.on_apply_baseline_editor(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let baseline = slider_value_to_baseline(
            app.get_baseline_editor_value(),
            app.get_baseline_editor_min(),
            app.get_baseline_editor_max(),
        );
        let value = if baseline <= 0.0 {
            "Natural".to_owned()
        } else {
            format!("{baseline:.0}")
        };
        set_building_baseline_label(&app, &app.get_baseline_editor_building().to_string(), value);
        app.set_baseline_editor_open(false);
    });
    let weak = app.as_weak();
    app.on_add_extra_target(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        add_or_update_extra_target(&app, &app.get_extra_target_selection().to_string());
        sync_extra_requirements_json(&app);
    });
    let weak = app.as_weak();
    app.on_remove_extra_target(move |index| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        remove_extra_target_at(&app, index);
        sync_extra_requirements_json(&app);
    });
    let weak = app.as_weak();
    app.on_load_scenario({
        let last_scenario_path = last_scenario_path.clone();
        move || {
            let Some(app) = weak.upgrade() else {
                return;
            };
            match load_scenario_config(&last_scenario_path) {
                Ok(config) => {
                    apply_scenario_to_ui(&app, &config);
                    app.set_status_text("Loaded scenario".into());
                }
                Err(error) => {
                    app.set_status_text("Load failed".into());
                    app.set_report_text(error.to_string().into());
                }
            }
        }
    });
    let weak = app.as_weak();
    app.on_save_scenario({
        let last_scenario_path = last_scenario_path.clone();
        move || {
            let Some(app) = weak.upgrade() else {
                return;
            };
            match scenario_from_ui(&app).and_then(|config| {
                save_scenario_config(&last_scenario_path, &config)
                    .map(|_| config)
                    .map_err(|error| error.to_string())
            }) {
                Ok(_) => app.set_status_text("Saved scenario".into()),
                Err(error) => {
                    app.set_status_text("Save failed".into());
                    app.set_report_text(error.into());
                }
            }
        }
    });
    let weak = app.as_weak();
    app.on_reset_scenario(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        apply_scenario_to_ui(&app, &default_scenario_config());
        app.set_status_text("Reset scenario".into());
    });
    let weak = app.as_weak();
    app.on_run_solver({
        let active_cancel = active_cancel.clone();
        move || {
        let Some(app) = weak.upgrade() else {
            return;
        };

        if app.get_solving() {
            if let Some(cancel) = active_cancel.lock().expect("cancel mutex").as_ref() {
                cancel.store(true, Ordering::Relaxed);
                app.set_status_text("Stopping phase 1, then finishing report...".into());
            }
            return;
        }

        match scenario_from_ui(&app) {
            Ok(config) => {
                if let Err(error) = save_scenario_config(&last_scenario_path, &config) {
                    app.set_status_text(format!("Could not save last scenario: {error}").into());
                }
                app.set_solving(true);
                app.set_status_text("Starting solve...".into());
                let cancel_requested = Arc::new(AtomicBool::new(false));
                *active_cancel.lock().expect("cancel mutex") = Some(cancel_requested.clone());
                let weak = app.as_weak();
                let progress_weak = app.as_weak();
                let progress_cancel = cancel_requested.clone();
                let progress_callback = Arc::new(move |progress: SolverProgress| {
                    let status = format_progress_status(&progress, progress_cancel.load(Ordering::Relaxed));
                    let progress_weak = progress_weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = progress_weak.upgrade() {
                            if app.get_solving() {
                                app.set_status_text(status.into());
                            }
                        }
                    });
                });
                let finish_cancel = active_cancel.clone();
                std::thread::spawn(move || {
                    let outcome = run_phase1_scenario_embedded_with_progress(
                        &config,
                        Some(progress_callback),
                        Some(cancel_requested),
                    )
                        .map(|result| {
                            let was_interrupted = result.phase1_interrupted;
                            (was_interrupted, format_phase1_report(&config, &result))
                        })
                        .map_err(|error| error.to_string());
                    let _ = slint::invoke_from_event_loop(move || {
                        *finish_cancel.lock().expect("cancel mutex") = None;
                        if let Some(app) = weak.upgrade() {
                            app.set_solving(false);
                            match outcome {
                                Ok((was_interrupted, report)) => {
                                    app.set_status_text(
                                        if was_interrupted {
                                            "Stopped after phase 1; report finished".into()
                                        } else {
                                            "Solved".into()
                                        }
                                    );
                                    app.set_report_text(report.into());
                                }
                                Err(error) => {
                                    app.set_status_text("Solve failed".into());
                                    app.set_report_text(error.into());
                                }
                            }
                        }
                    });
                });
            }
            Err(error) => {
                app.set_status_text("Invalid scenario".into());
                app.set_report_text(error.into());
            }
        }
    }});

    app.run().expect("UI should run");
}

fn default_scenario_config() -> ScenarioConfig {
    ScenarioConfig {
        building_counts: BTreeMap::from([
            (BuildingType::FarmT1, 0),
            (BuildingType::FarmT2, 2),
            (BuildingType::FarmT3, 0),
            (BuildingType::FarmT4, 0),
        ]),
        foods: vec!["Vegetables".to_owned(), "Tofu".to_owned()],
        food_multiplier: 1.0,
        extra_requirements: BTreeMap::new(),
        fertilizer: Some(FERTILIZER_ORGANIC),
        baseline_fertility_by_building: BTreeMap::from([
            (BuildingType::FarmT1, None),
            (BuildingType::FarmT2, Some(100.0)),
            (BuildingType::FarmT3, None),
            (BuildingType::FarmT4, None),
        ]),
    }
}

fn format_progress_status(progress: &SolverProgress, stopping: bool) -> String {
    let mut parts = vec![format!(
        "{} {:.1}s",
        if stopping { "Stopping phase 1..." } else { "Solving..." },
        progress.running_time_seconds
    )];
    if progress.best_population_estimate.is_finite() {
        parts.push(format!("best pop {:.1}", progress.best_population_estimate));
    }
    if progress.mip_gap.is_finite() {
        parts.push(format!("gap {:.2}%", progress.mip_gap * 100.0));
    }
    parts.push(format!("nodes {}", progress.mip_node_count));
    parts.join(" | ")
}
