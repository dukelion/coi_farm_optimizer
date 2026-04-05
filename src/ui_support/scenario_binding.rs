use std::collections::BTreeMap;

use coi_rust::domain::crop::BuildingType;
use coi_rust::domain::fertility::{
    FertilizerProduct, FERTILIZER_I, FERTILIZER_II, FERTILIZER_ORGANIC,
};
use coi_rust::scenario::ScenarioConfig;

use crate::AppWindow;
use crate::ui_support::extra_targets::{extra_requirements_from_ui, set_extra_target_rows};

pub fn apply_scenario_to_ui(app: &AppWindow, config: &ScenarioConfig) {
    app.set_farm_t1_count(config.building_counts.get(&BuildingType::FarmT1).copied().unwrap_or(0) as i32);
    app.set_farm_t2_count(config.building_counts.get(&BuildingType::FarmT2).copied().unwrap_or(0) as i32);
    app.set_farm_t3_count(config.building_counts.get(&BuildingType::FarmT3).copied().unwrap_or(0) as i32);
    app.set_farm_t4_count(config.building_counts.get(&BuildingType::FarmT4).copied().unwrap_or(0) as i32);

    app.set_food_potatoes(config.foods.iter().any(|food| food == "Potatoes"));
    app.set_food_corn(config.foods.iter().any(|food| food == "Corn"));
    app.set_food_bread(config.foods.iter().any(|food| food == "Bread"));
    app.set_food_vegetables(config.foods.iter().any(|food| food == "Vegetables"));
    app.set_food_tofu(config.foods.iter().any(|food| food == "Tofu"));
    app.set_food_meat(config.foods.iter().any(|food| food == "Meat"));
    app.set_food_eggs(config.foods.iter().any(|food| food == "Eggs"));
    app.set_food_sausage(config.foods.iter().any(|food| food == "Sausage"));
    app.set_food_fruit(config.foods.iter().any(|food| food == "Fruit"));
    app.set_food_snack(config.foods.iter().any(|food| food == "Snack"));
    app.set_food_cake(config.foods.iter().any(|food| food == "Cake"));

    app.set_food_multiplier_text(format!("{}", config.food_multiplier).into());
    set_extra_target_rows(app, &config.extra_requirements);
    app.set_fertilizer_selection(
        match config.fertilizer.map(|value| value.name) {
            None => "None",
            Some("Fertilizer (organic)") => "Organic Fert",
            Some(other) => other,
        }
        .into(),
    );
    app.set_baseline_farm_t1(format_baseline(config.baseline_fertility_by_building.get(&BuildingType::FarmT1).copied().flatten()).into());
    app.set_baseline_farm_t2(format_baseline(config.baseline_fertility_by_building.get(&BuildingType::FarmT2).copied().flatten()).into());
    app.set_baseline_farm_t3(format_baseline(config.baseline_fertility_by_building.get(&BuildingType::FarmT3).copied().flatten()).into());
    app.set_baseline_farm_t4(format_baseline(config.baseline_fertility_by_building.get(&BuildingType::FarmT4).copied().flatten()).into());
}

pub fn scenario_from_ui(app: &AppWindow) -> Result<ScenarioConfig, String> {
    let building_counts = BTreeMap::from([
        (BuildingType::FarmT1, app.get_farm_t1_count() as u32),
        (BuildingType::FarmT2, app.get_farm_t2_count() as u32),
        (BuildingType::FarmT3, app.get_farm_t3_count() as u32),
        (BuildingType::FarmT4, app.get_farm_t4_count() as u32),
    ]);

    let mut foods = Vec::new();
    push_if_checked(&mut foods, app.get_food_potatoes(), "Potatoes");
    push_if_checked(&mut foods, app.get_food_corn(), "Corn");
    push_if_checked(&mut foods, app.get_food_bread(), "Bread");
    push_if_checked(&mut foods, app.get_food_vegetables(), "Vegetables");
    push_if_checked(&mut foods, app.get_food_tofu(), "Tofu");
    push_if_checked(&mut foods, app.get_food_meat(), "Meat");
    push_if_checked(&mut foods, app.get_food_eggs(), "Eggs");
    push_if_checked(&mut foods, app.get_food_sausage(), "Sausage");
    push_if_checked(&mut foods, app.get_food_fruit(), "Fruit");
    push_if_checked(&mut foods, app.get_food_snack(), "Snack");
    push_if_checked(&mut foods, app.get_food_cake(), "Cake");

    if foods.is_empty() {
        return Err("Select at least one food.".to_owned());
    }

    let food_multiplier = app
        .get_food_multiplier_text()
        .to_string()
        .trim()
        .parse::<f64>()
        .map_err(|_| "Food multiplier must be a number.".to_owned())?;

    let extra_requirements = extra_requirements_from_ui(app)?;

    Ok(ScenarioConfig {
        building_counts,
        foods,
        food_multiplier,
        extra_requirements,
        fertilizer: parse_fertilizer_label(&app.get_fertilizer_selection().to_string())?,
        baseline_fertility_by_building: BTreeMap::from([
            (BuildingType::FarmT1, parse_baseline_label(&app.get_baseline_farm_t1().to_string())?),
            (BuildingType::FarmT2, parse_baseline_label(&app.get_baseline_farm_t2().to_string())?),
            (BuildingType::FarmT3, parse_baseline_label(&app.get_baseline_farm_t3().to_string())?),
            (BuildingType::FarmT4, parse_baseline_label(&app.get_baseline_farm_t4().to_string())?),
        ]),
    })
}

fn push_if_checked(foods: &mut Vec<String>, checked: bool, food: &str) {
    if checked {
        foods.push(food.to_owned());
    }
}

fn parse_fertilizer_label(value: &str) -> Result<Option<FertilizerProduct>, String> {
    match value {
        "None" => Ok(None),
        "Organic Fert" => Ok(Some(FERTILIZER_ORGANIC)),
        "Fertilizer I" => Ok(Some(FERTILIZER_I)),
        "Fertilizer II" => Ok(Some(FERTILIZER_II)),
        other => Err(format!("Unknown fertilizer option: {other}")),
    }
}

fn parse_baseline_label(value: &str) -> Result<Option<f64>, String> {
    match value {
        "Natural" => Ok(None),
        other => other
            .parse::<f64>()
            .map(Some)
            .map_err(|_| format!("Invalid baseline fertility: {other}")),
    }
}

fn format_baseline(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| "Natural".to_owned())
}
