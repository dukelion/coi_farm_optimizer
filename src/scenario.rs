use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::domain::crop::BuildingType;
use crate::domain::fertility::{FertilizerProduct, FertilityError};
use crate::domain::optimizer::{
    optimize_building_mix_mip, optimize_building_mix_mip_with_progress, OptimizationResult,
    OptimizerError, ProgressCallback,
};
use crate::io::recipe::{AuthoritativeRecipeData, RecipeLoadError};
use crate::io::wiki::{load_crop_catalog, load_embedded_crop_catalog, WikiLoadError};
use crate::scenario_prep::prepare_scenario;

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioConfig {
    pub building_counts: BTreeMap<BuildingType, u32>,
    pub foods: Vec<String>,
    pub food_multiplier: f64,
    pub extra_requirements: BTreeMap<String, f64>,
    pub fertilizer: Option<FertilizerProduct>,
    pub baseline_fertility_by_building: BTreeMap<BuildingType, Option<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioPaths {
    pub wiki_crop_data_path: PathBuf,
    pub machines_and_buildings_path: PathBuf,
}

impl Default for ScenarioPaths {
    fn default() -> Self {
        Self {
            wiki_crop_data_path: PathBuf::from("data/wiki_crop_data.json"),
            machines_and_buildings_path: PathBuf::from("captain-of-data/data/machines_and_buildings.json"),
        }
    }
}

#[derive(Debug)]
pub enum ScenarioError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Config(String),
    Wiki(WikiLoadError),
    Recipes(RecipeLoadError),
    Fertility(FertilityError),
    Optimization(OptimizerError),
}

impl Display for ScenarioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Config(error) => write!(f, "{error}"),
            Self::Wiki(error) => write!(f, "{error}"),
            Self::Recipes(error) => write!(f, "{error}"),
            Self::Fertility(error) => write!(f, "{error}"),
            Self::Optimization(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

impl From<std::io::Error> for ScenarioError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ScenarioError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<WikiLoadError> for ScenarioError {
    fn from(value: WikiLoadError) -> Self {
        Self::Wiki(value)
    }
}

impl From<RecipeLoadError> for ScenarioError {
    fn from(value: RecipeLoadError) -> Self {
        Self::Recipes(value)
    }
}

impl From<FertilityError> for ScenarioError {
    fn from(value: FertilityError) -> Self {
        Self::Fertility(value)
    }
}

impl From<OptimizerError> for ScenarioError {
    fn from(value: OptimizerError) -> Self {
        Self::Optimization(value)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct JsonScenarioConfig {
    building_counts: BTreeMap<String, u32>,
    foods: Vec<String>,
    #[serde(default = "default_food_multiplier")]
    food_multiplier: f64,
    #[serde(default)]
    extra_requirements: BTreeMap<String, f64>,
    fertilizer: Option<String>,
    #[serde(default)]
    baseline_fertility_by_building: BTreeMap<String, Option<f64>>,
}

fn default_food_multiplier() -> f64 {
    1.0
}

pub fn load_scenario_config(path: &Path) -> Result<ScenarioConfig, ScenarioError> {
    let text = fs::read_to_string(path)?;
    let json: JsonScenarioConfig = serde_json::from_str(&text)?;
    json_to_scenario_config(json)
}

pub fn save_scenario_config(path: &Path, config: &ScenarioConfig) -> Result<(), ScenarioError> {
    let json = scenario_config_to_json(config);
    let text = serde_json::to_string_pretty(&json)?;
    fs::write(path, text)?;
    Ok(())
}

fn json_to_scenario_config(json: JsonScenarioConfig) -> Result<ScenarioConfig, ScenarioError> {
    let building_counts = json
        .building_counts
        .into_iter()
        .map(|(building, count)| {
            BuildingType::from_str(&building)
                .map(|parsed| (parsed, count))
                .map_err(|error| ScenarioError::Config(error.to_string()))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let baseline_fertility_by_building = json
        .baseline_fertility_by_building
        .into_iter()
        .map(|(building, value)| {
            BuildingType::from_str(&building)
                .map(|parsed| (parsed, value))
                .map_err(|error| ScenarioError::Config(error.to_string()))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    Ok(ScenarioConfig {
        building_counts,
        foods: json.foods,
        food_multiplier: json.food_multiplier,
        extra_requirements: json.extra_requirements,
        fertilizer: parse_fertilizer(json.fertilizer.as_deref())?,
        baseline_fertility_by_building,
    })
}

fn scenario_config_to_json(config: &ScenarioConfig) -> JsonScenarioConfig {
    JsonScenarioConfig {
        building_counts: config
            .building_counts
            .iter()
            .map(|(building, count)| (building.as_str().to_owned(), *count))
            .collect(),
        foods: config.foods.clone(),
        food_multiplier: config.food_multiplier,
        extra_requirements: config.extra_requirements.clone(),
        fertilizer: config.fertilizer.map(|value| match value.name {
            "Fertilizer (organic)" => "Organic Fert".to_owned(),
            other => other.to_owned(),
        }),
        baseline_fertility_by_building: config
            .baseline_fertility_by_building
            .iter()
            .map(|(building, value)| (building.as_str().to_owned(), *value))
            .collect(),
    }
}

fn parse_fertilizer(name: Option<&str>) -> Result<Option<FertilizerProduct>, ScenarioError> {
    match name {
        None | Some("None") => Ok(None),
        Some("Organic Fert") | Some("Fertilizer (organic)") => Ok(Some(crate::domain::fertility::FERTILIZER_ORGANIC)),
        Some("Fertilizer I") => Ok(Some(crate::domain::fertility::FERTILIZER_I)),
        Some("Fertilizer II") => Ok(Some(crate::domain::fertility::FERTILIZER_II)),
        Some(other) => Err(ScenarioError::Config(format!("unknown fertilizer option: {other}"))),
    }
}

pub fn run_phase1_scenario(
    config: &ScenarioConfig,
    paths: &ScenarioPaths,
) -> Result<OptimizationResult, ScenarioError> {
    let crop_catalog = load_crop_catalog(Path::new(&paths.wiki_crop_data_path))?;
    let recipe_data = AuthoritativeRecipeData::load(Path::new(&paths.machines_and_buildings_path))?;
    let prepared = prepare_scenario(config, &crop_catalog, &recipe_data)?;
    let foods = prepared.foods.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(optimize_building_mix_mip(
        &prepared.building_pools,
        &foods,
        config.food_multiplier,
        &config.extra_requirements,
        &prepared.food_variants,
        &prepared.recipe_requirement_variants,
        &prepared.slack_sink_variants,
    )?)
}

pub fn run_phase1_scenario_embedded(
    config: &ScenarioConfig,
) -> Result<OptimizationResult, ScenarioError> {
    run_phase1_scenario_embedded_with_progress(config, None, None)
}

pub fn run_phase1_scenario_embedded_with_progress(
    config: &ScenarioConfig,
    progress_callback: Option<ProgressCallback>,
    cancel_requested: Option<Arc<AtomicBool>>,
) -> Result<OptimizationResult, ScenarioError> {
    let crop_catalog = load_embedded_crop_catalog()?;
    let recipe_data = AuthoritativeRecipeData::load_embedded()?;
    let prepared = prepare_scenario(config, &crop_catalog, &recipe_data)?;
    let foods = prepared.foods.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(optimize_building_mix_mip_with_progress(
        &prepared.building_pools,
        &foods,
        config.food_multiplier,
        &config.extra_requirements,
        &prepared.food_variants,
        &prepared.recipe_requirement_variants,
        &prepared.slack_sink_variants,
        progress_callback,
        cancel_requested,
    )?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    use crate::domain::crop::BuildingType;
    use crate::domain::fertility::FERTILIZER_ORGANIC;

    use super::{
        load_scenario_config, run_phase1_scenario, run_phase1_scenario_embedded, save_scenario_config,
        ScenarioConfig, ScenarioPaths,
    };

    #[test]
    fn run_phase1_scenario_solves_small_tofu_and_vegetables_case() {
        let result = run_phase1_scenario(
            &ScenarioConfig {
                building_counts: BTreeMap::from([(BuildingType::FarmT2, 2)]),
                foods: vec!["Vegetables".to_owned(), "Tofu".to_owned()],
                food_multiplier: 1.0,
                extra_requirements: BTreeMap::new(),
                fertilizer: Some(FERTILIZER_ORGANIC),
                baseline_fertility_by_building: BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
            },
            &ScenarioPaths::default(),
        )
        .expect("scenario should solve");

        assert!(result.supported_population > 0.0);
        assert_eq!(
            result.selected_options_by_building[&BuildingType::FarmT2]
                .iter()
                .map(|selection| selection.count)
                .sum::<u32>(),
            2
        );
    }

    #[test]
    fn embedded_scenario_path_solves_small_tofu_and_vegetables_case() {
        let result = run_phase1_scenario_embedded(&ScenarioConfig {
            building_counts: BTreeMap::from([(BuildingType::FarmT2, 2)]),
            foods: vec!["Vegetables".to_owned(), "Tofu".to_owned()],
            food_multiplier: 1.0,
            extra_requirements: BTreeMap::new(),
            fertilizer: Some(FERTILIZER_ORGANIC),
            baseline_fertility_by_building: BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
        })
        .expect("embedded scenario should solve");

        assert!(result.supported_population > 0.0);
    }

    #[test]
    fn load_scenario_config_parses_json_file() {
        let path = PathBuf::from("target/test-scenario.json");
        fs::create_dir_all(path.parent().expect("target dir should exist"))
            .expect("target dir should be creatable");
        fs::write(
            &path,
            r#"{
  "building_counts": { "FarmT2": 2 },
  "foods": ["Vegetables", "Tofu"],
  "food_multiplier": 1.0,
  "extra_requirements": { "Saplings": 2.0 },
  "fertilizer": "Organic Fert",
  "baseline_fertility_by_building": { "FarmT2": 100.0 }
}"#,
        )
        .expect("scenario file should be written");

        let config = load_scenario_config(&path).expect("scenario config should load");

        assert_eq!(config.building_counts[&BuildingType::FarmT2], 2);
        assert_eq!(config.foods, vec!["Vegetables".to_owned(), "Tofu".to_owned()]);
        assert_eq!(config.extra_requirements["Saplings"], 2.0);
        assert_eq!(config.fertilizer, Some(FERTILIZER_ORGANIC));
        assert_eq!(config.baseline_fertility_by_building[&BuildingType::FarmT2], Some(100.0));
    }

    #[test]
    fn save_scenario_config_writes_roundtrippable_json() {
        let path = PathBuf::from("target/test-scenario-roundtrip.json");
        let config = ScenarioConfig {
            building_counts: BTreeMap::from([(BuildingType::FarmT2, 2)]),
            foods: vec!["Vegetables".to_owned(), "Tofu".to_owned()],
            food_multiplier: 1.25,
            extra_requirements: BTreeMap::from([("Saplings".to_owned(), 2.0)]),
            fertilizer: Some(FERTILIZER_ORGANIC),
            baseline_fertility_by_building: BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
        };

        save_scenario_config(&path, &config).expect("scenario config should save");
        let loaded = load_scenario_config(&path).expect("scenario config should load");

        assert_eq!(loaded, config);
    }
}
