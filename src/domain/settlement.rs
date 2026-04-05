use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

fn food_categories() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("Potatoes", "Carbs"),
        ("Corn", "Carbs"),
        ("Bread", "Carbs"),
        ("Meat", "Protein"),
        ("Eggs", "Protein"),
        ("Tofu", "Protein"),
        ("Sausage", "Protein"),
        ("Vegetables", "Vitamins"),
        ("Fruit", "Vitamins"),
        ("Snack", "Treats"),
        ("Cake", "Treats"),
    ])
}

fn food_demand_factors() -> BTreeMap<&'static str, f64> {
    BTreeMap::from([
        ("Potatoes", 4.20),
        ("Corn", 3.00),
        ("Bread", 2.00),
        ("Meat", 2.70),
        ("Eggs", 3.00),
        ("Tofu", 1.80),
        ("Sausage", 3.35),
        ("Vegetables", 4.20),
        ("Fruit", 3.15),
        ("Snack", 2.60),
        ("Cake", 2.50),
    ])
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettlementFoodConsumption {
    pub population: u32,
    pub multiplier: f64,
    pub selected_foods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettlementError {
    NoValidFoodSelected,
}

impl Display for SettlementError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoValidFoodSelected => write!(f, "No valid food selected."),
        }
    }
}

impl std::error::Error for SettlementError {}

impl SettlementFoodConsumption {
    pub fn from_selected_foods(
        population: u32,
        multiplier: f64,
        selected_foods: &[&str],
    ) -> Result<Self, SettlementError> {
        let valid_foods = selected_foods
            .iter()
            .filter(|food| food_categories().contains_key(**food))
            .map(|food| (*food).to_owned())
            .collect::<Vec<_>>();
        if valid_foods.is_empty() {
            return Err(SettlementError::NoValidFoodSelected);
        }
        Ok(Self {
            population,
            multiplier,
            selected_foods: valid_foods,
        })
    }

    pub fn foods_by_category(&self) -> BTreeMap<String, Vec<String>> {
        let categories = food_categories();
        let mut grouped = BTreeMap::<String, Vec<String>>::new();
        for food in &self.selected_foods {
            if let Some(category) = categories.get(food.as_str()) {
                grouped
                    .entry((*category).to_owned())
                    .or_default()
                    .push(food.clone());
            }
        }
        grouped
    }

    pub fn categories_fulfilled(&self) -> usize {
        self.foods_by_category().len()
    }

    pub fn demand_per_100_for_food(&self, food: &str) -> f64 {
        if !self.selected_foods.iter().any(|selected| selected == food) {
            return 0.0;
        }
        let categories = food_categories();
        let category = categories[food];
        let foods_by_category = self.foods_by_category();
        let foods_in_category = foods_by_category[category].len() as f64;
        let base_demand = food_demand_factors()[food];
        base_demand / (self.categories_fulfilled() as f64 * foods_in_category)
    }

    pub fn monthly_demand_for_food(&self, food: &str) -> f64 {
        (self.population as f64 / 100.0) * self.demand_per_100_for_food(food) * self.multiplier
    }

    pub fn demand_by_food(&self) -> BTreeMap<String, f64> {
        self.selected_foods
            .iter()
            .map(|food| (food.clone(), round1(self.monthly_demand_for_food(food))))
            .collect()
    }

    pub fn total_monthly_demand(&self) -> f64 {
        round1(
            self.selected_foods
                .iter()
                .map(|food| self.monthly_demand_for_food(food))
                .sum(),
        )
    }

    pub fn supported_population_by_category(
        &self,
        produced_food_amounts: &BTreeMap<String, f64>,
    ) -> (BTreeMap<String, f64>, f64) {
        let mut category_population_supported = BTreeMap::new();
        for (category, foods) in self.foods_by_category() {
            let supported = foods
                .iter()
                .map(|food| {
                    let monthly_per_100 = self.demand_per_100_for_food(food) * self.multiplier;
                    let produced_food = produced_food_amounts.get(food).copied().unwrap_or(0.0);
                    if monthly_per_100 > 0.0 {
                        produced_food * 100.0 / monthly_per_100
                    } else {
                        0.0
                    }
                })
                .fold(f64::INFINITY, f64::min);
            category_population_supported.insert(category, supported);
        }

        let total_population_supported = category_population_supported
            .values()
            .copied()
            .fold(f64::INFINITY, f64::min);
        (
            category_population_supported,
            if total_population_supported.is_infinite() {
                0.0
            } else {
                total_population_supported
            },
        )
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{SettlementError, SettlementFoodConsumption};

    #[test]
    fn settlement_food_consumption_splits_demand_by_category_and_food_count() {
        let model = SettlementFoodConsumption::from_selected_foods(
            1000,
            1.0,
            &["Potatoes", "Corn", "Vegetables", "Tofu"],
        )
        .expect("model should build");

        assert_eq!(model.categories_fulfilled(), 3);
        assert!((model.demand_per_100_for_food("Potatoes") - 4.2 / 6.0).abs() < 1e-9);
        assert!((model.demand_per_100_for_food("Corn") - 3.0 / 6.0).abs() < 1e-9);
        assert!((model.demand_per_100_for_food("Vegetables") - 4.2 / 3.0).abs() < 1e-9);
        assert!((model.demand_per_100_for_food("Tofu") - 1.8 / 3.0).abs() < 1e-9);
        assert_eq!(
            model.demand_by_food(),
            BTreeMap::from([
                ("Corn".to_owned(), 5.0),
                ("Potatoes".to_owned(), 7.0),
                ("Tofu".to_owned(), 6.0),
                ("Vegetables".to_owned(), 14.0),
            ])
        );
    }

    #[test]
    fn supported_population_by_category_uses_minimum_food_in_each_category() {
        let model = SettlementFoodConsumption::from_selected_foods(
            100,
            1.0,
            &["Potatoes", "Corn", "Vegetables", "Tofu"],
        )
        .expect("model should build");

        let (category_supported, total_supported) = model.supported_population_by_category(
            &BTreeMap::from([
                ("Potatoes".to_owned(), 14.0),
                ("Corn".to_owned(), 10.0),
                ("Vegetables".to_owned(), 28.0),
                ("Tofu".to_owned(), 12.0),
            ]),
        );

        assert!((category_supported["Carbs"] - 2000.0).abs() < 1e-9);
        assert!((category_supported["Vitamins"] - 2000.0).abs() < 1e-9);
        assert!((category_supported["Protein"] - 2000.0).abs() < 1e-9);
        assert!((total_supported - 2000.0).abs() < 1e-9);
    }

    #[test]
    fn calculate_food_demand_scales_with_multiplier() {
        let model = SettlementFoodConsumption::from_selected_foods(
            1000,
            1.5,
            &["Vegetables", "Tofu"],
        )
        .expect("model should build");

        assert_eq!(
            model.demand_by_food(),
            BTreeMap::from([
                ("Tofu".to_owned(), 13.5),
                ("Vegetables".to_owned(), 31.5),
            ])
        );
    }

    #[test]
    fn from_selected_foods_rejects_invalid_selection() {
        let error = SettlementFoodConsumption::from_selected_foods(1000, 1.0, &["Invalid"])
            .expect_err("invalid selection should fail");
        assert_eq!(error, SettlementError::NoValidFoodSelected);
    }
}
