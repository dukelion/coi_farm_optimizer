use std::collections::BTreeSet;
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
    if runs.is_empty() {
        println!("  (none)");
        return;
    }

    for (name, amount) in runs {
        if *amount > 1e-9 {
            println!("  {name}: {amount:.4} runs/month");
        }
    }
}

fn print_diff(
    phase1: &std::collections::BTreeMap<String, f64>,
    phase2: &std::collections::BTreeMap<String, f64>,
) {
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
