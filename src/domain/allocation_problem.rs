use std::collections::{BTreeMap, BTreeSet};

use crate::domain::allocation::AllocationError;
use crate::domain::recipe::RecipeVariant;
use crate::domain::settlement::SettlementFoodConsumption;

pub(crate) struct PreparedAllocationProblem {
    pub available_crops: BTreeMap<String, f64>,
    pub food_requirement_rates: BTreeMap<String, f64>,
    pub recipe_requirements: BTreeMap<String, f64>,
    pub ordered_variants: Vec<RecipeVariant>,
}

pub(crate) fn prepare_allocation_problem(
    crop_outputs: &BTreeMap<String, f64>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<PreparedAllocationProblem, AllocationError> {
    let (direct_crop_requirements, recipe_requirements) =
        split_requirements(extra_requirements, recipe_requirement_variants);
    let available_crops = available_crops(crop_outputs, &direct_crop_requirements)?;

    let settlement = SettlementFoodConsumption::from_selected_foods(100, food_multiplier, filtered_foods)
        .map_err(|error| AllocationError::Settlement(error.to_string()))?;
    let food_requirement_rates = settlement
        .selected_foods
        .iter()
        .map(|food| {
            (
                food.clone(),
                (settlement.demand_per_100_for_food(food) * food_multiplier) / 100.0,
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut unique_variants = BTreeMap::<String, RecipeVariant>::new();
    for food in filtered_foods {
        if let Some(variants) = food_variants.get(*food) {
            for variant in variants
                .iter()
                .filter(|variant| uses_only_available_crops(variant, &available_crops))
            {
                unique_variants
                    .entry(variant.name.clone())
                    .or_insert_with(|| variant.clone());
            }
        }
    }
    for output in recipe_requirements.keys() {
        if let Some(variants) = recipe_requirement_variants.get(output) {
            for variant in variants
                .iter()
                .filter(|variant| uses_only_available_crops(variant, &available_crops))
            {
                unique_variants
                    .entry(variant.name.clone())
                    .or_insert_with(|| variant.clone());
            }
        }
    }
    for variant in slack_sink_variants
        .iter()
        .filter(|variant| uses_only_available_crops(variant, &available_crops))
    {
        unique_variants
            .entry(variant.name.clone())
            .or_insert_with(|| variant.clone());
    }

    Ok(PreparedAllocationProblem {
        available_crops,
        food_requirement_rates,
        recipe_requirements,
        ordered_variants: unique_variants.into_values().collect(),
    })
}

pub(crate) fn build_food_outputs(
    filtered_foods: &[&str],
    ordered_variants: &[RecipeVariant],
    process_runs: &BTreeMap<String, f64>,
) -> BTreeMap<String, f64> {
    filtered_foods
        .iter()
        .map(|food| {
            let amount = ordered_variants.iter().fold(0.0, |acc, variant| {
                acc + process_runs.get(&variant.name).copied().unwrap_or(0.0)
                    * variant.outputs.get(*food).copied().unwrap_or(0.0)
            });
            ((*food).to_owned(), amount)
        })
        .collect::<BTreeMap<_, _>>()
}

pub(crate) fn producible_crops_by_name(building_crop_keys: impl Iterator<Item = String>) -> BTreeSet<String> {
    building_crop_keys.collect()
}

pub(crate) fn exact_food_requirement_rates(
    filtered_foods: &[&str],
    food_multiplier: f64,
) -> Result<BTreeMap<String, f64>, AllocationError> {
    let settlement = SettlementFoodConsumption::from_selected_foods(100, food_multiplier, filtered_foods)
        .map_err(|error| AllocationError::Settlement(error.to_string()))?;
    Ok(settlement
        .selected_foods
        .iter()
        .map(|food| {
            (
                food.clone(),
                (settlement.demand_per_100_for_food(food) * food_multiplier) / 100.0,
            )
        })
        .collect())
}

pub(crate) fn split_requirements(
    extra_requirements: &BTreeMap<String, f64>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
) -> (BTreeMap<String, f64>, BTreeMap<String, f64>) {
    let direct_crop_requirements = extra_requirements
        .iter()
        .filter(|(name, _)| !recipe_requirement_variants.contains_key(name.as_str()))
        .map(|(name, amount)| (name.clone(), *amount))
        .collect::<BTreeMap<_, _>>();
    let recipe_requirements = extra_requirements
        .iter()
        .filter(|(name, _)| recipe_requirement_variants.contains_key(name.as_str()))
        .map(|(name, amount)| (name.clone(), *amount))
        .collect::<BTreeMap<_, _>>();
    (direct_crop_requirements, recipe_requirements)
}

fn available_crops(
    crop_outputs: &BTreeMap<String, f64>,
    direct_crop_requirements: &BTreeMap<String, f64>,
) -> Result<BTreeMap<String, f64>, AllocationError> {
    let mut available_crops = BTreeMap::new();
    for (crop_name, produced_amount) in crop_outputs {
        let remaining = produced_amount - direct_crop_requirements.get(crop_name).copied().unwrap_or(0.0);
        if remaining < -1e-6 {
            return Err(AllocationError::Infeasible);
        }
        if remaining > 1e-9 {
            available_crops.insert(crop_name.clone(), remaining);
        }
    }
    Ok(available_crops)
}

fn uses_only_available_crops(
    variant: &RecipeVariant,
    available_crops: &BTreeMap<String, f64>,
) -> bool {
    variant.inputs.keys().all(|crop| available_crops.contains_key(crop))
}
