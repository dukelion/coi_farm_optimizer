use std::path::Path;

use coi_rust::report::format_phase1_report;
use coi_rust::scenario::{load_scenario_config, run_phase1_scenario, ScenarioPaths};

fn main() {
    let scenario_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scenario.json".to_owned());
    let config = match load_scenario_config(Path::new(&scenario_path)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Failed to load scenario from {}: {}", scenario_path, error);
            std::process::exit(1);
        }
    };

    match run_phase1_scenario(&config, &ScenarioPaths::default()) {
        Ok(result) => {
            println!("{}", format_phase1_report(&config, &result));
        }
        Err(error) => {
            eprintln!("Scenario solve failed: {error}");
            std::process::exit(1);
        }
    }
}
