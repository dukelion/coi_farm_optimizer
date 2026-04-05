use std::collections::BTreeMap;

use crate::domain::catalog::build_baseline_catalog_by_building;
use crate::domain::crop::{BuildingType, CropCatalog};
use crate::domain::optimizer::BuildingPool;
use crate::domain::recipe::RecipeVariant;
use crate::io::recipe::AuthoritativeRecipeData;
use crate::scenario::{ScenarioConfig, ScenarioError};

pub(crate) struct PreparedScenario {
    pub building_pools: BTreeMap<BuildingType, BuildingPool>,
    pub foods: Vec<String>,
    pub food_variants: BTreeMap<String, Vec<RecipeVariant>>,
    pub recipe_requirement_variants: BTreeMap<String, Vec<RecipeVariant>>,
    pub slack_sink_variants: Vec<RecipeVariant>,
}

pub(crate) fn prepare_scenario(
    config: &ScenarioConfig,
    crop_catalog: &CropCatalog,
    recipe_data: &AuthoritativeRecipeData,
) -> Result<PreparedScenario, ScenarioError> {
    let food_variants = recipe_data.build_food_variants()?;
    let recipe_requirement_variants = recipe_data.build_requirement_variants()?;
    let slack_sink_variants = recipe_data.build_slack_sink_variants()?;

    let crops = crops_needed_for_scenario(config, &food_variants, &recipe_requirement_variants);
    let baseline_catalog = build_baseline_catalog_by_building(
        crop_catalog,
        &crops,
        &config.baseline_fertility_by_building,
        config.fertilizer,
    )?;

    let building_pools = config
        .building_counts
        .iter()
        .filter_map(|(building, count)| {
            if *count == 0 {
                return None;
            }
            let options = baseline_catalog.get(building).cloned().unwrap_or_default();
            Some((
                *building,
                BuildingPool {
                    farm_count: *count,
                    options,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    Ok(PreparedScenario {
        building_pools,
        foods: config.foods.clone(),
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    })
}

fn crops_needed_for_scenario(
    config: &ScenarioConfig,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
) -> Vec<String> {
    let mut crops = config
        .foods
        .iter()
        .flat_map(|food| {
            food_variants
                .get(food)
                .into_iter()
                .flat_map(|variants| variants.iter())
                .flat_map(|variant| variant.inputs.keys().cloned())
        })
        .collect::<Vec<_>>();

    for (requirement, amount) in &config.extra_requirements {
        if *amount <= 0.0 {
            continue;
        }
        if let Some(variants) = recipe_requirement_variants.get(requirement) {
            crops.extend(
                variants
                    .iter()
                    .flat_map(|variant| variant.inputs.keys().cloned()),
            );
        } else {
            crops.push(requirement.clone());
        }
    }

    crops.sort();
    crops.dedup();
    crops
}
