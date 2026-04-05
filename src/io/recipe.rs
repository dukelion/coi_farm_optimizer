use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::domain::recipe::{LoadedRecipe, RecipeVariant};

const RELEVANT_RECIPE_IDS: &[&str] = &[
    "WheatMilling",
    "CornMilling",
    "BreadProduction",
    "SoybeanMilling",
    "CanolaMilling",
    "TofuProduction",
    "FoodPackAssemblyTofuT2",
    "FoodPackAssemblyEggsT2",
    "FoodPackAssemblyMeatT2",
    "SausageProduction",
    "SnackProductionPotato",
    "SnackProductionCorn",
    "AnimalFeedFromPotato",
    "AnimalFeedFromWheat",
    "AnimalFeedFromCorn",
    "AnimalFeedFromSoybean",
    "AnimalFeedCompost",
    "ChickenFarm",
    "MeatProcessing",
    "MeatTrimmingsCompost",
    "SugarRefiningCane",
    "SugarToEthanolFermentation",
    "CornToEthanolFermentation",
    "CakeProduction",
    "BiomassCompost",
    "PotatoDigestion",
    "VegetablesDigestion",
    "FruitDigestion",
    "SugarCaneDigestion",
];

pub const FULL_CHICKEN_FARM_SIZE: f64 = 500.0;

fn material_aliases() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("Animal feed", "Animal Feed"),
        ("Biomass", "Biomass"),
        ("Bread", "Bread"),
        ("Cake", "Cake"),
        ("Canola", "Canola"),
        ("Chicken carcass", "Chicken Carcass"),
        ("Compost", "Compost"),
        ("Cooking oil", "Cooking Oil"),
        ("Corn", "Corn"),
        ("Corn mash", "Corn Mash"),
        ("Ethanol", "Ethanol"),
        ("Eggs", "Eggs"),
        ("Food pack", "Food Pack"),
        ("Flour", "Flour"),
        ("Fruit", "Fruit"),
        ("Meat", "Meat"),
        ("Meat trimmings", "Meat Trimmings"),
        ("Potato", "Potatoes"),
        ("Sausage", "Sausage"),
        ("Snack", "Snack"),
        ("Sugar", "Sugar"),
        ("Sugar cane", "Sugar Cane"),
        ("Poppy", "Poppy"),
        ("Tree sapling", "Saplings"),
        ("Soybean", "Soybean"),
        ("Tofu", "Tofu"),
        ("Vegetables", "Vegetables"),
        ("Wheat", "Wheat"),
    ])
}

fn exposed_materials() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "Animal Feed",
        "Biomass",
        "Bread",
        "Cake",
        "Canola",
        "Chicken Carcass",
        "Compost",
        "Cooking Oil",
        "Corn",
        "Ethanol",
        "Eggs",
        "Food Pack",
        "Fruit",
        "Meat",
        "Meat Trimmings",
        "Potatoes",
        "Sausage",
        "Snack",
        "Sugar",
        "Sugar Cane",
        "Poppy",
        "Saplings",
        "Soybean",
        "Tofu",
        "Vegetables",
        "Wheat",
    ])
}

#[derive(Debug)]
pub enum RecipeLoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingRecipeIds(Vec<String>),
    MissingRecipe(String),
}

impl Display for RecipeLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::MissingRecipeIds(ids) => write!(f, "missing authoritative recipes: {:?}", ids),
            Self::MissingRecipe(id) => write!(f, "missing recipe: {id}"),
        }
    }
}

impl std::error::Error for RecipeLoadError {}

impl From<std::io::Error> for RecipeLoadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for RecipeLoadError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Deserialize)]
struct MachinePayload {
    machines_and_buildings: Vec<MachineEntry>,
}

#[derive(Debug, Deserialize)]
struct MachineEntry {
    recipes: Option<Vec<RecipeEntry>>,
}

#[derive(Debug, Deserialize)]
struct RecipeEntry {
    id: String,
    name: String,
    inputs: Option<Vec<RecipeIoEntry>>,
    outputs: Option<Vec<RecipeIoEntry>>,
}

#[derive(Debug, Deserialize)]
struct RecipeIoEntry {
    name: String,
    quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuthoritativeRecipeData {
    pub recipes_by_id: BTreeMap<String, LoadedRecipe>,
}

impl AuthoritativeRecipeData {
    pub fn load(path: &Path) -> Result<Self, RecipeLoadError> {
        let text = fs::read_to_string(path)?;
        Self::load_from_str(&text)
    }

    pub fn load_embedded() -> Result<Self, RecipeLoadError> {
        Self::load_from_str(include_str!("../../captain-of-data/data/machines_and_buildings.json"))
    }

    fn load_from_str(text: &str) -> Result<Self, RecipeLoadError> {
        let payload: MachinePayload = serde_json::from_str(&text)?;
        let relevant: BTreeSet<&str> = RELEVANT_RECIPE_IDS.iter().copied().collect();
        let mut recipes_by_id = BTreeMap::new();

        for machine in payload.machines_and_buildings {
            for recipe in machine.recipes.unwrap_or_default() {
                if !relevant.contains(recipe.id.as_str()) {
                    continue;
                }
                recipes_by_id.insert(
                    recipe.id.clone(),
                    LoadedRecipe {
                        id: recipe.id,
                        name: recipe.name,
                        inputs: normalize_io(recipe.inputs.unwrap_or_default()),
                        outputs: normalize_io(recipe.outputs.unwrap_or_default()),
                    },
                );
            }
        }

        let missing = RELEVANT_RECIPE_IDS
            .iter()
            .filter(|id| !recipes_by_id.contains_key(**id))
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(RecipeLoadError::MissingRecipeIds(missing));
        }

        Ok(Self { recipes_by_id })
    }

    pub fn recipe(&self, recipe_id: &str) -> Result<&LoadedRecipe, RecipeLoadError> {
        self.recipes_by_id
            .get(recipe_id)
            .ok_or_else(|| RecipeLoadError::MissingRecipe(recipe_id.to_owned()))
    }

    fn combine_steps(
        &self,
        name: &str,
        steps: &[(&str, f64)],
    ) -> Result<RecipeVariant, RecipeLoadError> {
        let exposed = exposed_materials();
        let mut net_materials: BTreeMap<String, f64> = BTreeMap::new();

        for (recipe_id, scale) in steps {
            let recipe = self.recipe(recipe_id)?;
            for (material, quantity) in &recipe.outputs {
                *net_materials.entry(material.clone()).or_insert(0.0) += quantity * scale;
            }
            for (material, quantity) in &recipe.inputs {
                *net_materials.entry(material.clone()).or_insert(0.0) -= quantity * scale;
            }
        }

        let inputs = net_materials
            .iter()
            .filter(|(material, quantity)| **quantity < -1e-9 && exposed.contains(material.as_str()))
            .map(|(material, quantity)| (material.clone(), -quantity))
            .collect::<BTreeMap<_, _>>();
        let outputs = net_materials
            .iter()
            .filter(|(material, quantity)| **quantity > 1e-9 && exposed.contains(material.as_str()))
            .map(|(material, quantity)| (material.clone(), *quantity))
            .collect::<BTreeMap<_, _>>();

        Ok(RecipeVariant {
            name: name.to_owned(),
            inputs,
            outputs,
            primary_output: None,
            animal_feed_source: None,
            animal_feed_used: 0.0,
            chicken_farm_runs: 0.0,
            chicken_eggs_produced: 0.0,
            chicken_carcasses_produced: 0.0,
        })
    }

    pub fn build_requirement_variants(
        &self,
    ) -> Result<BTreeMap<String, Vec<RecipeVariant>>, RecipeLoadError> {
        let wheat_milling = self.recipe("WheatMilling")?;
        let bread_recipe = self.recipe("BreadProduction")?;
        let eggs_pack_recipe = self.recipe("FoodPackAssemblyEggsT2")?;
        let meat_pack_recipe = self.recipe("FoodPackAssemblyMeatT2")?;
        let meat_processing = self.recipe("MeatProcessing")?;
        let chicken_farm = self.recipe("ChickenFarm")?;
        let sugar_to_ethanol = self.recipe("SugarToEthanolFermentation")?;
        let corn_to_ethanol = self.recipe("CornToEthanolFermentation")?;

        let tofu_pack = self.combine_steps(
            "Tofu Pack",
            &[("TofuProduction", 0.75), ("FoodPackAssemblyTofuT2", 1.0)],
        )?;

        let feed_recipe_ids = [
            ("Potatoes", "AnimalFeedFromPotato"),
            ("Wheat", "AnimalFeedFromWheat"),
            ("Corn", "AnimalFeedFromCorn"),
            ("Soybean", "AnimalFeedFromSoybean"),
        ];

        let mut eggs_packs = Vec::new();
        let mut meat_packs = Vec::new();
        let mut balanced_chicken_packs = Vec::new();

        for (crop_name, feed_recipe_id) in feed_recipe_ids {
            let animal_feed_output = self.recipe(feed_recipe_id)?.outputs["Animal Feed"];

            let eggs_pack_scale = 1.0 / eggs_pack_recipe.outputs["Food Pack"];
            let eggs_chain_scale = eggs_pack_recipe.inputs["Eggs"]
                * eggs_pack_scale
                / chicken_farm.outputs["Eggs"];
            let eggs_feed_scale = eggs_chain_scale * chicken_farm.inputs["Animal Feed"]
                / animal_feed_output;
            let mut eggs_pack = self.combine_steps(
                &format!("Eggs Pack ({crop_name})"),
                &[
                    (
                        "WheatMilling",
                        eggs_pack_recipe.inputs["Bread"]
                            * eggs_pack_scale
                            * bread_recipe.inputs["Flour"]
                            / bread_recipe.outputs["Bread"]
                            / wheat_milling.outputs["Flour"],
                    ),
                    (
                        "BreadProduction",
                        eggs_pack_recipe.inputs["Bread"] * eggs_pack_scale / bread_recipe.outputs["Bread"],
                    ),
                    (feed_recipe_id, eggs_feed_scale),
                    ("ChickenFarm", eggs_chain_scale),
                    ("FoodPackAssemblyEggsT2", eggs_pack_scale),
                ],
            )?;
            eggs_pack.animal_feed_source = Some(crop_name.to_owned());
            eggs_pack.animal_feed_used = eggs_chain_scale * chicken_farm.inputs["Animal Feed"];
            eggs_pack.chicken_farm_runs = eggs_chain_scale;
            eggs_pack.chicken_eggs_produced = eggs_chain_scale * chicken_farm.outputs["Eggs"];
            eggs_pack.chicken_carcasses_produced = eggs_chain_scale * chicken_farm.outputs["Chicken Carcass"];
            eggs_packs.push(eggs_pack);

            let meat_pack_scale = 1.0 / meat_pack_recipe.outputs["Food Pack"];
            let meat_process_scale = meat_pack_recipe.inputs["Meat"]
                * meat_pack_scale
                / self.recipe("MeatProcessing")?.outputs["Meat"];
            let meat_feed_scale = meat_process_scale * chicken_farm.inputs["Animal Feed"]
                / animal_feed_output;
            let mut meat_pack = self.combine_steps(
                &format!("Meat Pack ({crop_name})"),
                &[
                    (
                        "WheatMilling",
                        meat_pack_recipe.inputs["Bread"]
                            * meat_pack_scale
                            * bread_recipe.inputs["Flour"]
                            / bread_recipe.outputs["Bread"]
                            / wheat_milling.outputs["Flour"],
                    ),
                    (
                        "BreadProduction",
                        meat_pack_recipe.inputs["Bread"] * meat_pack_scale / bread_recipe.outputs["Bread"],
                    ),
                    (feed_recipe_id, meat_feed_scale),
                    ("ChickenFarm", meat_process_scale),
                    ("MeatProcessing", meat_process_scale),
                    ("FoodPackAssemblyMeatT2", meat_pack_scale),
                ],
            )?;
            meat_pack.animal_feed_source = Some(crop_name.to_owned());
            meat_pack.animal_feed_used = meat_process_scale * chicken_farm.inputs["Animal Feed"];
            meat_pack.chicken_farm_runs = meat_process_scale;
            meat_pack.chicken_eggs_produced = meat_process_scale * chicken_farm.outputs["Eggs"];
            meat_pack.chicken_carcasses_produced = meat_process_scale * chicken_farm.outputs["Chicken Carcass"];
            meat_packs.push(meat_pack);

            let egg_pack_runs_from_one_chicken =
                chicken_farm.outputs["Eggs"] / eggs_pack_recipe.inputs["Eggs"];
            let meat_processing_runs_from_one_chicken =
                chicken_farm.outputs["Chicken Carcass"] / meat_processing.inputs["Chicken Carcass"];
            let meat_pack_runs_from_one_chicken =
                meat_processing_runs_from_one_chicken * meat_processing.outputs["Meat"]
                    / meat_pack_recipe.inputs["Meat"];
            let total_food_pack_output_from_one_chicken = egg_pack_runs_from_one_chicken
                * eggs_pack_recipe.outputs["Food Pack"]
                + meat_pack_runs_from_one_chicken * meat_pack_recipe.outputs["Food Pack"];
            let chicken_scale = 1.0 / total_food_pack_output_from_one_chicken;
            let bread_runs = (egg_pack_runs_from_one_chicken * eggs_pack_recipe.inputs["Bread"]
                + meat_pack_runs_from_one_chicken * meat_pack_recipe.inputs["Bread"])
                * chicken_scale
                / bread_recipe.outputs["Bread"];
            let wheat_runs =
                bread_runs * bread_recipe.inputs["Flour"] / wheat_milling.outputs["Flour"];
            let eggs_pack_runs = egg_pack_runs_from_one_chicken * chicken_scale;
            let meat_processing_runs = meat_processing_runs_from_one_chicken * chicken_scale;
            let meat_pack_runs = meat_pack_runs_from_one_chicken * chicken_scale;
            let feed_runs = chicken_scale * chicken_farm.inputs["Animal Feed"] / animal_feed_output;
            let mut balanced_pack = self.combine_steps(
                &format!("Balanced Food Pack ({crop_name})"),
                &[
                    ("WheatMilling", wheat_runs),
                    ("BreadProduction", bread_runs),
                    (feed_recipe_id, feed_runs),
                    ("ChickenFarm", chicken_scale),
                    ("MeatProcessing", meat_processing_runs),
                    ("FoodPackAssemblyEggsT2", eggs_pack_runs),
                    ("FoodPackAssemblyMeatT2", meat_pack_runs),
                ],
            )?;
            balanced_pack.primary_output = Some("Food Pack".to_owned());
            balanced_pack.animal_feed_source = Some(crop_name.to_owned());
            balanced_pack.animal_feed_used = chicken_scale * chicken_farm.inputs["Animal Feed"];
            balanced_pack.chicken_farm_runs = chicken_scale;
            balanced_pack.chicken_eggs_produced = chicken_scale * chicken_farm.outputs["Eggs"];
            balanced_pack.chicken_carcasses_produced =
                chicken_scale * chicken_farm.outputs["Chicken Carcass"];
            balanced_chicken_packs.push(balanced_pack);
        }

        let cooking_oil_variants = dedupe_variants(vec![
            {
                let mut variant = self.combine_steps(
                    "Cooking Oil (Soybean)",
                    &[("SoybeanMilling", 1.0 / self.recipe("SoybeanMilling")?.outputs["Cooking Oil"])],
                )?;
                variant.primary_output = Some("Cooking Oil".to_owned());
                variant
            },
            {
                let mut variant = self.combine_steps(
                    "Cooking Oil (Canola)",
                    &[("CanolaMilling", 1.0 / self.recipe("CanolaMilling")?.outputs["Cooking Oil"])],
                )?;
                variant.primary_output = Some("Cooking Oil".to_owned());
                variant
            },
        ]);

        let sugar_variants = dedupe_variants(vec![{
            let mut variant = self.combine_steps(
                "Sugar (Cane)",
                &[("SugarRefiningCane", 1.0 / self.recipe("SugarRefiningCane")?.outputs["Sugar"])],
            )?;
            variant.primary_output = Some("Sugar".to_owned());
            variant
        }]);

        let ethanol_variants = dedupe_variants(vec![
            {
                let mut variant = self.combine_steps(
                    "Ethanol (Sugar)",
                    &[
                        (
                            "SugarRefiningCane",
                            sugar_to_ethanol.inputs["Sugar"] / self.recipe("SugarRefiningCane")?.outputs["Sugar"],
                        ),
                        ("SugarToEthanolFermentation", 1.0 / sugar_to_ethanol.outputs["Ethanol"]),
                    ],
                )?;
                variant.primary_output = Some("Ethanol".to_owned());
                variant
            },
            {
                let mut variant = self.combine_steps(
                    "Ethanol (Corn)",
                    &[
                        (
                            "CornMilling",
                            corn_to_ethanol.inputs["Corn Mash"] / self.recipe("CornMilling")?.outputs["Corn Mash"],
                        ),
                        ("CornToEthanolFermentation", 1.0 / corn_to_ethanol.outputs["Ethanol"]),
                    ],
                )?;
                variant.primary_output = Some("Ethanol".to_owned());
                variant
            },
        ]);

        Ok(BTreeMap::from([
            (
                "Food Pack".to_owned(),
                dedupe_variants({
                    let mut variants = vec![tofu_pack];
                    variants.extend(eggs_packs);
                    variants.extend(meat_packs);
                    variants.extend(balanced_chicken_packs);
                    variants
                }),
            ),
            ("Cooking Oil".to_owned(), cooking_oil_variants),
            ("Sugar".to_owned(), sugar_variants),
            ("Ethanol".to_owned(), ethanol_variants),
        ]))
    }

    pub fn build_food_variants(&self) -> Result<BTreeMap<String, Vec<RecipeVariant>>, RecipeLoadError> {
        let wheat_milling = self.recipe("WheatMilling")?;
        let bread_recipe = self.recipe("BreadProduction")?;
        let tofu_recipe = self.recipe("TofuProduction")?;
        let sausage_recipe = self.recipe("SausageProduction")?;
        let snack_potato_recipe = self.recipe("SnackProductionPotato")?;
        let snack_corn_recipe = self.recipe("SnackProductionCorn")?;
        let sugar_refining_cane = self.recipe("SugarRefiningCane")?;
        let biomass_compost = self.recipe("BiomassCompost")?;
        let cake_recipe = self.recipe("CakeProduction")?;
        let chicken_farm = self.recipe("ChickenFarm")?;

        let direct = |food: &str| RecipeVariant {
            name: format!("Direct {food}"),
            inputs: BTreeMap::from([(food.to_owned(), 1.0)]),
            outputs: BTreeMap::from([(food.to_owned(), 1.0)]),
            primary_output: Some(food.to_owned()),
            animal_feed_source: None,
            animal_feed_used: 0.0,
            chicken_farm_runs: 0.0,
            chicken_eggs_produced: 0.0,
            chicken_carcasses_produced: 0.0,
        };

        let feed_recipe_ids = [
            ("Potatoes", "AnimalFeedFromPotato"),
            ("Wheat", "AnimalFeedFromWheat"),
            ("Corn", "AnimalFeedFromCorn"),
            ("Soybean", "AnimalFeedFromSoybean"),
        ];
        let chicken_increment_scale = 0.1;
        let animal_feed_per_increment = chicken_farm.inputs["Animal Feed"] * chicken_increment_scale;

        let mut egg_variants = Vec::new();
        let mut meat_variants = Vec::new();
        let mut sausage_variants = Vec::new();
        let mut snack_variants = Vec::new();
        let mut cake_variants = Vec::new();

        for (crop_name, feed_recipe_id) in feed_recipe_ids {
            let animal_feed_output = self.recipe(feed_recipe_id)?.outputs["Animal Feed"];
            let feed_scale = animal_feed_per_increment / animal_feed_output;

            let mut egg_variant = self.combine_steps(
                &format!("Egg Production ({crop_name})"),
                &[(feed_recipe_id, feed_scale), ("ChickenFarm", chicken_increment_scale)],
            )?;
            egg_variant.primary_output = Some("Eggs".to_owned());
            egg_variant.animal_feed_source = Some(crop_name.to_owned());
            egg_variant.animal_feed_used = chicken_increment_scale * chicken_farm.inputs["Animal Feed"];
            egg_variant.chicken_farm_runs = chicken_increment_scale;
            egg_variant.chicken_eggs_produced = chicken_increment_scale * chicken_farm.outputs["Eggs"];
            egg_variant.chicken_carcasses_produced = chicken_increment_scale * chicken_farm.outputs["Chicken Carcass"];
            egg_variants.push(egg_variant);

            let mut meat_variant = self.combine_steps(
                &format!("Meat Production ({crop_name})"),
                &[
                    (feed_recipe_id, feed_scale),
                    ("ChickenFarm", chicken_increment_scale),
                    ("MeatProcessing", chicken_increment_scale),
                    ("MeatTrimmingsCompost", chicken_increment_scale / 6.0),
                ],
            )?;
            meat_variant.primary_output = Some("Meat".to_owned());
            meat_variant.animal_feed_source = Some(crop_name.to_owned());
            meat_variant.animal_feed_used = chicken_increment_scale * chicken_farm.inputs["Animal Feed"];
            meat_variant.chicken_farm_runs = chicken_increment_scale;
            meat_variant.chicken_eggs_produced = chicken_increment_scale * chicken_farm.outputs["Eggs"];
            meat_variant.chicken_carcasses_produced = chicken_increment_scale * chicken_farm.outputs["Chicken Carcass"];
            meat_variants.push(meat_variant);

            let sausage_scale = 1.0 / sausage_recipe.outputs["Sausage"];
            let meat_processing_scale = sausage_recipe.inputs["Meat Trimmings"]
                * sausage_scale
                / self.recipe("MeatProcessing")?.outputs["Meat Trimmings"];
            let mut sausage_variant = self.combine_steps(
                &format!("Sausage Production ({crop_name})"),
                &[
                    (
                        "WheatMilling",
                        sausage_recipe.inputs["Flour"] * sausage_scale / wheat_milling.outputs["Flour"],
                    ),
                    (
                        feed_recipe_id,
                        meat_processing_scale * self.recipe("ChickenFarm")?.inputs["Animal Feed"]
                            / animal_feed_output,
                    ),
                    ("ChickenFarm", meat_processing_scale),
                    ("MeatProcessing", meat_processing_scale),
                    ("SausageProduction", sausage_scale),
                ],
            )?;
            sausage_variant.primary_output = Some("Sausage".to_owned());
            sausage_variant.animal_feed_source = Some(crop_name.to_owned());
            sausage_variant.animal_feed_used = meat_processing_scale * chicken_farm.inputs["Animal Feed"];
            sausage_variant.chicken_farm_runs = meat_processing_scale;
            sausage_variant.chicken_eggs_produced = meat_processing_scale * chicken_farm.outputs["Eggs"];
            sausage_variant.chicken_carcasses_produced = meat_processing_scale * chicken_farm.outputs["Chicken Carcass"];
            sausage_variants.push(sausage_variant);

            for (oil_recipe_id, oil_label) in [("SoybeanMilling", "Soybean Oil"), ("CanolaMilling", "Canola Oil")] {
                let oil_recipe = self.recipe(oil_recipe_id)?;
                for (snack_recipe, snack_crop) in [
                    (snack_potato_recipe, "Potatoes"),
                    (snack_corn_recipe, "Corn"),
                ] {
                    let snack_scale = 1.0 / snack_recipe.outputs["Snack"];
                    let snack_biomass_to_compost_scale =
                        snack_scale * snack_recipe.outputs["Biomass"] / biomass_compost.inputs["Biomass"];
                    let mut snack_variant = self.combine_steps(
                        &format!("Snack Production ({snack_crop}, {oil_label})"),
                        &[
                            (
                                oil_recipe_id,
                                snack_recipe.inputs["Cooking Oil"] * snack_scale
                                    / oil_recipe.outputs["Cooking Oil"],
                            ),
                            (
                                if snack_crop == "Potatoes" {
                                    "SnackProductionPotato"
                                } else {
                                    "SnackProductionCorn"
                                },
                                snack_scale,
                            ),
                            ("BiomassCompost", snack_biomass_to_compost_scale),
                        ],
                    )?;
                    snack_variant.primary_output = Some("Snack".to_owned());
                    snack_variants.push(snack_variant);
                }

                let cake_scale = 1.0 / cake_recipe.outputs["Cake"];
                let sugar_scale = cake_recipe.inputs["Sugar"] * cake_scale / sugar_refining_cane.outputs["Sugar"];
                let biomass_to_compost_scale =
                    sugar_scale * sugar_refining_cane.outputs["Biomass"] / biomass_compost.inputs["Biomass"];
                let oil_scale =
                    cake_recipe.inputs["Cooking Oil"] * cake_scale / oil_recipe.outputs["Cooking Oil"];
                let egg_scale = cake_recipe.inputs["Eggs"] * cake_scale / chicken_farm.outputs["Eggs"];
                let mut cake_variant = self.combine_steps(
                    &format!("Cake Production ({crop_name}, {oil_label})"),
                    &[
                        (
                            "WheatMilling",
                            cake_recipe.inputs["Flour"] * cake_scale / wheat_milling.outputs["Flour"],
                        ),
                        ("SugarRefiningCane", sugar_scale),
                        ("BiomassCompost", biomass_to_compost_scale),
                        (oil_recipe_id, oil_scale),
                        (
                            feed_recipe_id,
                            egg_scale * chicken_farm.inputs["Animal Feed"] / animal_feed_output,
                        ),
                        ("ChickenFarm", egg_scale),
                        ("CakeProduction", cake_scale),
                    ],
                )?;
                cake_variant.primary_output = Some("Cake".to_owned());
                cake_variant.animal_feed_source = Some(crop_name.to_owned());
                cake_variant.animal_feed_used = egg_scale * chicken_farm.inputs["Animal Feed"];
                cake_variant.chicken_farm_runs = egg_scale;
                cake_variant.chicken_eggs_produced = egg_scale * chicken_farm.outputs["Eggs"];
                cake_variant.chicken_carcasses_produced = egg_scale * chicken_farm.outputs["Chicken Carcass"];
                cake_variants.push(cake_variant);
            }
        }

        Ok(BTreeMap::from([
            ("Potatoes".to_owned(), vec![direct("Potatoes")]),
            ("Corn".to_owned(), vec![direct("Corn")]),
            ("Vegetables".to_owned(), vec![direct("Vegetables")]),
            ("Fruit".to_owned(), vec![direct("Fruit")]),
            (
                "Bread".to_owned(),
                vec![{
                    let mut variant = self.combine_steps(
                        "Bread Production",
                        &[
                            (
                                "WheatMilling",
                                bread_recipe.inputs["Flour"]
                                    / bread_recipe.outputs["Bread"]
                                    / wheat_milling.outputs["Flour"],
                            ),
                            ("BreadProduction", 1.0 / bread_recipe.outputs["Bread"]),
                        ],
                    )?;
                    variant.primary_output = Some("Bread".to_owned());
                    variant
                }],
            ),
            (
                "Tofu".to_owned(),
                vec![{
                    let mut variant = self.combine_steps(
                        "Tofu Production",
                        &[("TofuProduction", 1.0 / tofu_recipe.outputs["Tofu"])],
                    )?;
                    variant.primary_output = Some("Tofu".to_owned());
                    variant
                }],
            ),
            ("Eggs".to_owned(), dedupe_variants({
                let mut variants = egg_variants.clone();
                variants.extend(meat_variants.clone());
                variants
            })),
            ("Meat".to_owned(), dedupe_variants(meat_variants)),
            ("Sausage".to_owned(), dedupe_variants(sausage_variants)),
            ("Snack".to_owned(), dedupe_variants(snack_variants)),
            ("Cake".to_owned(), dedupe_variants(cake_variants)),
        ]))
    }

    pub fn build_slack_sink_variants(&self) -> Result<Vec<RecipeVariant>, RecipeLoadError> {
        let mut variants = Vec::new();

        for (crop_name, recipe_id, output_name) in [
            ("Potatoes", "PotatoDigestion", "Compost"),
            ("Vegetables", "VegetablesDigestion", "Compost"),
            ("Fruit", "FruitDigestion", "Compost"),
            ("Sugar Cane", "SugarCaneDigestion", "Compost"),
            ("Potatoes", "AnimalFeedFromPotato", "Animal Feed"),
            ("Wheat", "AnimalFeedFromWheat", "Animal Feed"),
            ("Corn", "AnimalFeedFromCorn", "Animal Feed"),
            ("Soybean", "AnimalFeedFromSoybean", "Animal Feed"),
        ] {
            let recipe = self.recipe(recipe_id)?;
            let input = recipe.inputs.get(crop_name).copied().unwrap_or(0.0);
            let output = recipe.outputs.get(output_name).copied().unwrap_or(0.0);
            if input <= 0.0 || output <= 0.0 {
                continue;
            }
            variants.push(RecipeVariant {
                name: format!("Slack {} -> {}", crop_name, output_name),
                inputs: BTreeMap::from([(crop_name.to_owned(), input)]),
                outputs: BTreeMap::from([(output_name.to_owned(), output)]),
                primary_output: Some(output_name.to_owned()),
                animal_feed_source: None,
                animal_feed_used: 0.0,
                chicken_farm_runs: 0.0,
                chicken_eggs_produced: 0.0,
                chicken_carcasses_produced: 0.0,
            });
        }

        Ok(dedupe_variants(variants))
    }
}

fn normalize_io(entries: Vec<RecipeIoEntry>) -> BTreeMap<String, f64> {
    let aliases = material_aliases();
    entries
        .into_iter()
        .filter_map(|entry| {
            aliases
                .get(entry.name.as_str())
                .map(|material| ((*material).to_owned(), entry.quantity))
        })
        .collect()
}

fn rounded_signature_map(map: &BTreeMap<String, f64>) -> Vec<(String, i64)> {
    map.iter()
        .map(|(key, value)| (key.clone(), (value * 1_000_000.0).round() as i64))
        .collect()
}

fn variant_signature(variant: &RecipeVariant) -> (Vec<(String, i64)>, Vec<(String, i64)>, Option<String>) {
    (
        rounded_signature_map(&variant.inputs),
        rounded_signature_map(&variant.outputs),
        variant.primary_output.clone(),
    )
}

fn dedupe_variants(variants: Vec<RecipeVariant>) -> Vec<RecipeVariant> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for variant in variants {
        let signature = variant_signature(&variant);
        if seen.insert(signature) {
            deduped.push(variant);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use super::{dedupe_variants, variant_signature, AuthoritativeRecipeData};
    use crate::domain::recipe::RecipeVariant;

    fn source_path() -> PathBuf {
        PathBuf::from("captain-of-data/data/machines_and_buildings.json")
    }

    #[test]
    fn authoritative_recipe_loader_contains_food_pack_recipe_ids() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");

        assert!(data.recipes_by_id.contains_key("FoodPackAssemblyTofuT2"));
        assert!(data.recipes_by_id.contains_key("FoodPackAssemblyEggsT2"));
        assert!(data.recipes_by_id.contains_key("FoodPackAssemblyMeatT2"));
    }

    #[test]
    fn requirement_variants_include_all_food_pack_paths() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_requirement_variants()
            .expect("variants should build");
        let food_pack_variants = &variants["Food Pack"];
        let names = food_pack_variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(food_pack_variants.len(), 13);
        assert_eq!(
            names,
            BTreeSet::from([
                "Tofu Pack",
                "Eggs Pack (Potatoes)",
                "Eggs Pack (Wheat)",
                "Eggs Pack (Corn)",
                "Eggs Pack (Soybean)",
                "Meat Pack (Potatoes)",
                "Meat Pack (Wheat)",
                "Meat Pack (Corn)",
                "Meat Pack (Soybean)",
                "Balanced Food Pack (Potatoes)",
                "Balanced Food Pack (Wheat)",
                "Balanced Food Pack (Corn)",
                "Balanced Food Pack (Soybean)",
            ])
        );
    }

    #[test]
    fn requirement_variants_include_extra_non_food_targets() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_requirement_variants()
            .expect("variants should build");

        assert_eq!(variants["Cooking Oil"].len(), 2);
        assert_eq!(variants["Sugar"].len(), 1);
        assert_eq!(variants["Ethanol"].len(), 2);
        assert!(variants["Cooking Oil"]
            .iter()
            .any(|variant| variant.name == "Cooking Oil (Soybean)"));
        assert!(variants["Cooking Oil"]
            .iter()
            .any(|variant| variant.name == "Cooking Oil (Canola)"));
        assert_eq!(variants["Sugar"][0].name, "Sugar (Cane)");
        assert!(variants["Ethanol"]
            .iter()
            .any(|variant| variant.name == "Ethanol (Sugar)"));
        assert!(variants["Ethanol"]
            .iter()
            .any(|variant| variant.name == "Ethanol (Corn)"));
    }

    #[test]
    fn tofu_pack_variant_matches_authoritative_chain_outputs() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_requirement_variants()
            .expect("variants should build");
        let tofu_pack = variants["Food Pack"]
            .iter()
            .find(|variant| variant.name == "Tofu Pack")
            .expect("tofu pack should exist");

        assert_eq!(tofu_pack.inputs["Soybean"], 4.5);
        assert_eq!(tofu_pack.inputs["Vegetables"], 8.0);
        assert_eq!(tofu_pack.outputs["Animal Feed"], 2.25);
        assert_eq!(tofu_pack.outputs["Food Pack"], 4.0);
    }

    #[test]
    fn wheat_eggs_pack_flattens_to_crop_only_input_and_food_pack_output() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_requirement_variants()
            .expect("variants should build");
        let eggs_pack = variants["Food Pack"]
            .iter()
            .find(|variant| variant.name == "Eggs Pack (Wheat)")
            .expect("wheat eggs pack should exist");

        assert_eq!(
            eggs_pack.inputs.keys().cloned().collect::<Vec<_>>(),
            vec!["Wheat".to_owned()]
        );
        assert!((eggs_pack.inputs["Wheat"] - 1.9687500469).abs() < 1e-6);
        assert!((eggs_pack.outputs["Food Pack"] - 1.0).abs() < 1e-9);
        assert!(eggs_pack.outputs["Chicken Carcass"] > 1.0);
        assert!((eggs_pack.outputs["Animal Feed"] - 0.125).abs() < 1e-9);
    }

    #[test]
    fn balanced_food_pack_consumes_carcasses_internally() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_requirement_variants()
            .expect("variants should build");
        let balanced_pack = variants["Food Pack"]
            .iter()
            .find(|variant| variant.name == "Balanced Food Pack (Corn)")
            .expect("balanced corn food pack should exist");

        assert!((balanced_pack.outputs["Food Pack"] - 1.0).abs() < 1e-9);
        assert!(balanced_pack.outputs.get("Chicken Carcass").copied().unwrap_or(0.0).abs() < 1e-9);
        assert!(balanced_pack.inputs.contains_key("Corn"));
        assert!(balanced_pack.inputs.contains_key("Wheat"));
    }

    #[test]
    fn dedupe_variants_removes_identical_flattened_chains() {
        let base = RecipeVariant {
            name: "Variant A".to_owned(),
            inputs: BTreeMap::from([("Wheat".to_owned(), 1.0)]),
            outputs: BTreeMap::from([("Food Pack".to_owned(), 1.0)]),
            primary_output: Some("Food Pack".to_owned()),
            animal_feed_source: None,
            animal_feed_used: 0.0,
            chicken_farm_runs: 0.0,
            chicken_eggs_produced: 0.0,
            chicken_carcasses_produced: 0.0,
        };
        let same_signature = RecipeVariant {
            name: "Variant B".to_owned(),
            inputs: BTreeMap::from([("Wheat".to_owned(), 1.0)]),
            outputs: BTreeMap::from([("Food Pack".to_owned(), 1.0)]),
            primary_output: Some("Food Pack".to_owned()),
            animal_feed_source: None,
            animal_feed_used: 0.0,
            chicken_farm_runs: 0.0,
            chicken_eggs_produced: 0.0,
            chicken_carcasses_produced: 0.0,
        };
        let different = RecipeVariant {
            name: "Variant C".to_owned(),
            inputs: BTreeMap::from([("Corn".to_owned(), 1.0)]),
            outputs: BTreeMap::from([("Food Pack".to_owned(), 1.0)]),
            primary_output: Some("Food Pack".to_owned()),
            animal_feed_source: None,
            animal_feed_used: 0.0,
            chicken_farm_runs: 0.0,
            chicken_eggs_produced: 0.0,
            chicken_carcasses_produced: 0.0,
        };

        let deduped = dedupe_variants(vec![base.clone(), same_signature, different.clone()]);

        assert_eq!(deduped.len(), 2);
        assert_eq!(variant_signature(&deduped[0]), variant_signature(&base));
        assert_eq!(variant_signature(&deduped[1]), variant_signature(&different));
    }

    #[test]
    fn food_variants_include_direct_foods_and_processed_basics() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data.build_food_variants().expect("food variants should build");

        assert_eq!(variants["Potatoes"][0].name, "Direct Potatoes");
        assert_eq!(variants["Potatoes"][0].primary_output.as_deref(), Some("Potatoes"));
        assert_eq!(variants["Corn"][0].name, "Direct Corn");
        assert_eq!(variants["Vegetables"][0].name, "Direct Vegetables");
        assert_eq!(variants["Fruit"][0].name, "Direct Fruit");
        assert_eq!(variants["Bread"][0].primary_output.as_deref(), Some("Bread"));
        assert_eq!(variants["Tofu"][0].primary_output.as_deref(), Some("Tofu"));
    }

    #[test]
    fn bread_variant_flattens_to_wheat_input() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data.build_food_variants().expect("food variants should build");
        let bread = &variants["Bread"][0];

        assert_eq!(
            bread.inputs.keys().cloned().collect::<Vec<_>>(),
            vec!["Wheat".to_owned()]
        );
        assert!((bread.inputs["Wheat"] - 0.6666666667).abs() < 1e-6);
        assert!((bread.outputs["Bread"] - 1.0).abs() < 1e-9);
        assert!(bread.outputs["Animal Feed"] > 0.0);
    }

    #[test]
    fn tofu_variant_flattens_to_soybean_input() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data.build_food_variants().expect("food variants should build");
        let tofu = &variants["Tofu"][0];

        assert_eq!(
            tofu.inputs.keys().cloned().collect::<Vec<_>>(),
            vec!["Soybean".to_owned()]
        );
        assert!((tofu.inputs["Soybean"] - 0.75).abs() < 1e-9);
        assert!((tofu.outputs["Tofu"] - 1.0).abs() < 1e-9);
        assert!(tofu.outputs["Animal Feed"] > 0.0);
    }

    #[test]
    fn food_variant_counts_match_expected_deduped_shapes() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data.build_food_variants().expect("food variants should build");

        assert_eq!(variants["Eggs"].len(), 8);
        assert_eq!(variants["Meat"].len(), 4);
        assert_eq!(variants["Sausage"].len(), 4);
        assert_eq!(variants["Snack"].len(), 4);
        assert_eq!(variants["Cake"].len(), 8);
    }

    #[test]
    fn slack_sink_variants_include_compost_and_animal_feed_sinks() {
        let data = AuthoritativeRecipeData::load(&source_path()).expect("recipe data should load");
        let variants = data
            .build_slack_sink_variants()
            .expect("slack sinks should build");
        let names = variants.iter().map(|variant| variant.name.as_str()).collect::<BTreeSet<_>>();

        assert!(names.contains("Slack Potatoes -> Compost"));
        assert!(names.contains("Slack Vegetables -> Compost"));
        assert!(names.contains("Slack Corn -> Animal Feed"));
        assert!(names.contains("Slack Wheat -> Animal Feed"));
    }
}
