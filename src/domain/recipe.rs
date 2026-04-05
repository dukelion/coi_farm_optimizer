use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedRecipe {
    pub id: String,
    pub name: String,
    pub inputs: BTreeMap<String, f64>,
    pub outputs: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecipeVariant {
    pub name: String,
    pub inputs: BTreeMap<String, f64>,
    pub outputs: BTreeMap<String, f64>,
    pub primary_output: Option<String>,
    pub animal_feed_source: Option<String>,
    pub animal_feed_used: f64,
    pub chicken_farm_runs: f64,
    pub chicken_eggs_produced: f64,
    pub chicken_carcasses_produced: f64,
}
