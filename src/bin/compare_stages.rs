use std::collections::BTreeSet;
use std::collections::BTreeMap;
use std::env;
use std::path::Path;

use coi_rust::domain::allocation::compare_allocation_stages_from_crop_outputs;
use coi_rust::io::recipe::AuthoritativeRecipeData;
use coi_rust::scenario::{load_scenario_config, run_phase1_scenario_embedded};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenario_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "last_scenario.json".to_owned());
    let config = load_scenario_config(Path::new(&scenario_path))?;
    let optimization = run_phase1_scenario_embedded(&config)?;

    let recipe_data = AuthoritativeRecipeData::load_embedded()?;
    let food_variants = recipe_data.build_food_variants()?;
    let requirement_variants = recipe_data.build_requirement_variants()?;
    let slack_sink_variants = recipe_data.build_slack_sink_variants()?;
    let foods = config.foods.iter().map(String::as_str).collect::<Vec<_>>();

    let comparison = compare_allocation_stages_from_crop_outputs(
        &optimization.crop_outputs,
        &foods,
        config.food_multiplier,
        &config.extra_requirements,
        &food_variants,
        &requirement_variants,
        &slack_sink_variants,
    )?;

    println!("Scenario: {}", Path::new(&scenario_path).display());
    println!("Phase 1 max population: {:.3}", comparison.phase1_max_population);
    println!(
        "Phase 2 stabilized population: {:.3}",
        comparison.stabilized_population
    );
    println!();
    println!("Phase 1 process runs:");
    print_runs(&comparison.phase1.process_runs);
    println!();
    println!("Phase 2 process runs:");
    print_runs(&comparison.phase2.process_runs);
    println!();
    println!("Changed chains:");
    print_diff(&comparison.phase1.process_runs, &comparison.phase2.process_runs);

    Ok(())
}

fn print_runs(runs: &std::collections::BTreeMap<String, f64>) {
    let display_runs = summarize_runs_for_display(runs);

    if display_runs.is_empty() {
        println!("  (none)");
        return;
    }

    for (name, amount) in display_runs {
        println!("  {name}: {amount:.4} runs/month");
    }
}

fn print_diff(
    phase1: &std::collections::BTreeMap<String, f64>,
    phase2: &std::collections::BTreeMap<String, f64>,
) {
    let phase1 = summarize_runs_for_display(phase1);
    let phase2 = summarize_runs_for_display(phase2);
    let keys = phase1
        .keys()
        .chain(phase2.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut changed = false;
    for key in keys {
        let a = phase1.get(&key).copied().unwrap_or(0.0);
        let b = phase2.get(&key).copied().unwrap_or(0.0);
        if (a - b).abs() > 1e-6 {
            changed = true;
            println!("  {key}: phase1={a:.4}, phase2={b:.4}, delta={:.4}", b - a);
        }
    }

    if !changed {
        println!("  (no changes)");
    }
}

fn summarize_runs_for_display(runs: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    const BALANCED_FOOD_PACK_EGG_SHARE: f64 = 0.594456940152;
    const BALANCED_FOOD_PACK_MEAT_SHARE: f64 = 0.405543059848;

    let mut display_runs = BTreeMap::new();

    for (name, amount) in runs {
        if *amount <= 1e-9 {
            continue;
        }

        if name.starts_with("Eggs Pack (") {
            *display_runs
                .entry("Food Pack (egg-based)".to_owned())
                .or_insert(0.0) += amount;
            continue;
        }

        if name.starts_with("Meat Pack (") {
            *display_runs
                .entry("Food Pack (meat-based)".to_owned())
                .or_insert(0.0) += amount;
            continue;
        }

        if name == "Tofu Pack" {
            *display_runs
                .entry("Food Pack (tofu-based)".to_owned())
                .or_insert(0.0) += amount;
            continue;
        }

        if name.starts_with("Balanced Food Pack (") {
            *display_runs
                .entry("Food Pack (egg-based)".to_owned())
                .or_insert(0.0) += amount * BALANCED_FOOD_PACK_EGG_SHARE;
            *display_runs
                .entry("Food Pack (meat-based)".to_owned())
                .or_insert(0.0) += amount * BALANCED_FOOD_PACK_MEAT_SHARE;
            continue;
        }

        *display_runs.entry(name.clone()).or_insert(0.0) += amount;
    }

    display_runs
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::summarize_runs_for_display;

    #[test]
    fn display_summary_hides_balanced_food_pack_internal_name() {
        let runs = BTreeMap::from([
            ("Balanced Food Pack (Corn)".to_owned(), 10.0),
            ("Eggs Pack (Wheat)".to_owned(), 1.0),
            ("Meat Pack (Corn)".to_owned(), 2.0),
            ("Tofu Pack".to_owned(), 0.5),
        ]);

        let display = summarize_runs_for_display(&runs);

        assert!(!display.keys().any(|name| name.starts_with("Balanced Food Pack (")));
        assert_eq!(display["Food Pack (tofu-based)"], 0.5);
        assert!((display["Food Pack (egg-based)"] - 6.94456940152).abs() < 1e-9);
        assert!((display["Food Pack (meat-based)"] - 6.05543059848).abs() < 1e-9);
    }
}
