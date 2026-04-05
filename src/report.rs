use minijinja::{context, Environment};
use serde::Serialize;

use crate::domain::crop::BuildingType;
use crate::domain::settlement::SettlementFoodConsumption;
use crate::scenario::ScenarioConfig;

const PHASE1_REPORT_TEMPLATE: &str = r#"Phase 1 Optimization Results

Foods: {{ foods }}
Food Multiplier: {{ food_multiplier }}
Fertilizer: {{ fertilizer }}
Supported Population: {{ supported_population }}
{%- if bottleneck_food %}
Bottleneck Food: {{ bottleneck_food.name }} ({{ bottleneck_food.produced }}/{{ bottleneck_food.required }} at target)
{%- endif %}
{%- if bottleneck_crop %}
Bottleneck Crop: {{ bottleneck_crop.name }} ({{ bottleneck_crop.used }}/{{ bottleneck_crop.available }} used)
{%- endif %}
Total Fertilizer Usage: {{ total_fertilizer_usage }}/month
Total Added Water Usage: {{ total_water_usage }}/month
{%- if extra_requirements %}

Extra Requirements:
{%- for item in extra_requirements %}
  {{ item.name }}: {{ item.amount }}/month
{%- endfor %}
{%- endif %}
{%- if chicken_summary %}

Chicken Farms:
  Chickens Needed: {{ chicken_summary.chickens_needed }} ({{ chicken_summary.full_farms_needed }} farms)
  Eggs Produced: {{ chicken_summary.eggs_produced }}/month
  Chicken Carcasses Produced: {{ chicken_summary.carcasses_produced }}/month
  Animal Feed Sources:
{%- for item in chicken_summary.animal_feed_sources %}
    {{ item.name }}: {{ item.amount }}/month
{%- endfor %}
{%- endif %}

Selected Rotations:
{%- for rotation in rotations %}
  {{ rotation.count }} x {{ rotation.building }} | {{ rotation.rotation }} | Fertility: {{ rotation.equilibrium }} -> {{ rotation.target }} ({{ rotation.fertilizer }}/mo)
{%- endfor %}

Crop Output:
{%- for item in crop_output %}
  {{ item.name }}: {{ item.amount }}/month
{%- endfor %}

Food Output:
{%- for item in food_output %}
  {{ item.name }}: {{ item.amount }}/month
{%- endfor %}
{%- if extra_output %}

Extra Output:
{%- for item in extra_output %}
  {{ item.name }}: {{ item.amount }}/month
{%- endfor %}
{%- endif %}
{%- if process_runs %}

Recipe Chains Used:
{%- for item in process_runs %}
  {{ item.name }}: {{ item.amount }} runs/month
{%- endfor %}
{%- endif %}
"#;

#[derive(Serialize)]
struct ReportItem {
    name: String,
    amount: String,
}

#[derive(Serialize)]
struct RotationItem {
    count: u32,
    building: &'static str,
    rotation: String,
    equilibrium: String,
    target: String,
    fertilizer: String,
}

#[derive(Serialize)]
struct BottleneckFoodView {
    name: String,
    produced: String,
    required: String,
}

#[derive(Serialize)]
struct BottleneckCropView {
    name: String,
    used: String,
    available: String,
}

#[derive(Serialize)]
struct ChickenSummaryView {
    animal_feed_sources: Vec<ReportItem>,
    full_farms_needed: String,
    chickens_needed: String,
    eggs_produced: String,
    carcasses_produced: String,
}

pub fn format_phase1_report(
    config: &ScenarioConfig,
    result: &crate::domain::optimizer::OptimizationResult,
) -> String {
    let mut env = Environment::new();
    env.add_template("phase1", PHASE1_REPORT_TEMPLATE)
        .expect("phase1 report template should parse");

    let foods = if config.foods.is_empty() {
        "None".to_owned()
    } else {
        config.foods.join(", ")
    };
    let fertilizer = config
        .fertilizer
        .map(|product| product.name.to_owned())
        .unwrap_or_else(|| "None".to_owned());

    let rotations = BuildingType::ALL
        .iter()
        .flat_map(|building| {
            result
                .selected_options_by_building
                .get(building)
                .into_iter()
                .flat_map(move |selections| {
                    selections.iter().map(move |selection| RotationItem {
                        count: selection.count,
                        building: building_label(*building),
                        rotation: selection.option.rotation.0.join(" -> "),
                        equilibrium: format!("{:.1}%", selection.option.simulation.fertility_equilibrium),
                        target: selection
                            .option
                            .fertility_target
                            .map(|value| format!("{value:.0}%"))
                            .unwrap_or_else(|| "Natural".to_owned()),
                        fertilizer: format!("{:.2}", selection.option.simulation.fertilizer_required_per_month),
                    })
                })
        })
        .collect::<Vec<_>>();

    let crop_output = result
        .crop_outputs
        .iter()
        .map(|(name, amount)| ReportItem {
            name: name.clone(),
            amount: format!("{amount:.2}"),
        })
        .collect::<Vec<_>>();

    let food_output = result
        .allocation
        .food_outputs
        .iter()
        .map(|(name, amount)| ReportItem {
            name: name.clone(),
            amount: format!("{amount:.2}"),
        })
        .collect::<Vec<_>>();

    let extra_output = result
        .allocation
        .extra_outputs
        .iter()
        .filter(|(_, amount)| **amount > 1e-9)
        .map(|(name, amount)| ReportItem {
            name: name.clone(),
            amount: format!("{amount:.2}"),
        })
        .collect::<Vec<_>>();

    let process_runs = result
        .allocation
        .process_runs
        .iter()
        .map(|(name, amount)| ReportItem {
            name: name.clone(),
            amount: format!("{amount:.2}"),
        })
        .collect::<Vec<_>>();

    let extra_requirements = config
        .extra_requirements
        .iter()
        .map(|(name, amount)| ReportItem {
            name: name.clone(),
            amount: format!("{amount:.2}"),
        })
        .collect::<Vec<_>>();

    let bottleneck_food = bottleneck_food(config, result).map(|(name, produced, required)| {
        BottleneckFoodView { name, produced, required }
    });
    let bottleneck_crop = bottleneck_crop(result).map(|(name, used, available)| BottleneckCropView {
        name,
        used: format!("{used:.2}"),
        available: format!("{available:.2}"),
    });
    let chicken_summary = result.allocation.chicken_summary.as_ref().map(|summary| ChickenSummaryView {
        animal_feed_sources: summary
            .animal_feed_sources
            .iter()
            .map(|(name, amount)| ReportItem {
                name: name.clone(),
                amount: format!("{amount:.2}"),
            })
            .collect(),
        full_farms_needed: format!("{:.2}", summary.full_farms_needed),
        chickens_needed: format!("{:.2}", summary.chickens_needed),
        eggs_produced: format!("{:.2}", summary.eggs_produced),
        carcasses_produced: format!("{:.2}", summary.carcasses_produced),
    });

    env.get_template("phase1")
        .expect("phase1 report template should exist")
        .render(context! {
            foods,
            food_multiplier => format!("{:.2}", config.food_multiplier),
            fertilizer,
            supported_population => format!("{:.1}", result.supported_population),
            bottleneck_food,
            bottleneck_crop,
            chicken_summary,
            total_fertilizer_usage => format!("{:.2}", result.total_fertilizer_per_month),
            total_water_usage => format!("{:.2}", result.total_water_per_month),
            extra_requirements,
            rotations,
            crop_output,
            food_output,
            extra_output,
            process_runs,
        })
        .expect("phase1 report template should render")
}

fn building_label(building: BuildingType) -> &'static str {
    match building {
        BuildingType::FarmT1 => "Farm",
        BuildingType::FarmT2 => "Irrigated Farm",
        BuildingType::FarmT3 => "Greenhouse",
        BuildingType::FarmT4 => "Greenhouse II",
    }
}

fn bottleneck_food(
    config: &ScenarioConfig,
    result: &crate::domain::optimizer::OptimizationResult,
) -> Option<(String, String, String)> {
    let foods = config.foods.iter().map(String::as_str).collect::<Vec<_>>();
    let settlement =
        SettlementFoodConsumption::from_selected_foods(100, config.food_multiplier, &foods).ok()?;

    config
        .foods
        .iter()
        .filter_map(|food| {
            let monthly_per_100 = settlement.demand_per_100_for_food(food) * config.food_multiplier;
            let produced_food = result.allocation.food_outputs.get(food).copied().unwrap_or(0.0);
            let required_food = (result.supported_population / 100.0) * monthly_per_100;
            (monthly_per_100 > 0.0).then(|| {
                (
                    food.clone(),
                    produced_food,
                    required_food,
                    produced_food - required_food,
                )
            })
        })
        .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(food, produced, required, _)| {
            (
                food,
                format!("{produced:.2}"),
                format!("{required:.2}"),
            )
        })
}

fn bottleneck_crop(
    result: &crate::domain::optimizer::OptimizationResult,
) -> Option<(String, f64, f64)> {
    result
        .crop_outputs
        .iter()
        .filter_map(|(crop, produced)| {
            let used = result
                .allocation
                .crop_inputs_used
                .get(crop)
                .copied()
                .unwrap_or(0.0);
            (used > 1e-9 && *produced > 1e-9).then(|| (crop.clone(), used, *produced, used / produced))
        })
        .max_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(crop, used, produced, _)| (crop, used, produced))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::domain::crop::BuildingType;
    use crate::domain::fertility::FERTILIZER_ORGANIC;
    use crate::scenario::{run_phase1_scenario, ScenarioConfig, ScenarioPaths};

    use super::format_phase1_report;

    #[test]
    fn formatted_report_includes_core_sections() {
        let config = ScenarioConfig {
            building_counts: BTreeMap::from([(BuildingType::FarmT2, 2)]),
            foods: vec!["Vegetables".to_owned(), "Tofu".to_owned()],
            food_multiplier: 1.0,
            extra_requirements: BTreeMap::new(),
            fertilizer: Some(FERTILIZER_ORGANIC),
            baseline_fertility_by_building: BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
        };
        let result = run_phase1_scenario(&config, &ScenarioPaths::default())
            .expect("scenario should solve");

        let report = format_phase1_report(&config, &result);

        assert!(report.contains("Phase 1 Optimization Results"));
        assert!(report.contains("Supported Population:"));
        assert!(report.contains("Selected Rotations:"));
        assert!(report.contains("Crop Output:"));
        assert!(report.contains("Food Output:"));
        assert!(report.contains("Bottleneck Food:"));
        assert!(report.contains("Bottleneck Crop:"));
        assert!(report.contains("Irrigated Farm"));
        assert!(report.contains("Soybean"));
        assert!(report.contains("Vegetables"));
    }

    #[test]
    fn formatted_report_includes_chicken_section_when_chickens_are_used() {
        let config = ScenarioConfig {
            building_counts: BTreeMap::from([(BuildingType::FarmT2, 2)]),
            foods: vec!["Eggs".to_owned()],
            food_multiplier: 1.0,
            extra_requirements: BTreeMap::new(),
            fertilizer: Some(FERTILIZER_ORGANIC),
            baseline_fertility_by_building: BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
        };
        let result = run_phase1_scenario(&config, &ScenarioPaths::default())
            .expect("scenario should solve");

        let report = format_phase1_report(&config, &result);

        assert!(report.contains("Chicken Farms:"));
        assert!(report.contains("Chickens Needed:"));
        assert!(report.contains(" farms)"));
        assert!(report.contains("Eggs Produced:"));
        assert!(report.contains("Chicken Carcasses Produced:"));
        assert!(report.contains("Animal Feed Sources:"));
    }
}
