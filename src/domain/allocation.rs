use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use good_lp::{
    default_solver, variable, variables, Expression, ProblemVariables, Solution, SolverModel,
    Variable,
};

use crate::domain::allocation_problem::{build_food_outputs, prepare_allocation_problem};
use crate::domain::recipe::RecipeVariant;
use crate::io::recipe::FULL_CHICKEN_FARM_SIZE;

#[derive(Debug, Clone, PartialEq)]
pub struct AllocationResult {
    pub supported_population: f64,
    pub food_outputs: BTreeMap<String, f64>,
    pub extra_outputs: BTreeMap<String, f64>,
    pub process_runs: BTreeMap<String, f64>,
    pub crop_inputs_used: BTreeMap<String, f64>,
    pub crop_inputs_remaining: BTreeMap<String, f64>,
    pub chicken_summary: Option<ChickenSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChickenSummary {
    pub animal_feed_sources: BTreeMap<String, f64>,
    pub full_farms_needed: f64,
    pub chickens_needed: f64,
    pub eggs_produced: f64,
    pub carcasses_produced: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AllocationStageComparison {
    pub phase1: AllocationResult,
    pub phase2: AllocationResult,
    pub phase1_max_population: f64,
    pub stabilized_population: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AllocationError {
    Settlement(String),
    Infeasible,
    Solver(String),
}

impl Display for AllocationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Settlement(message) => write!(f, "{message}"),
            Self::Infeasible => write!(f, "allocation is infeasible"),
            Self::Solver(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AllocationError {}

struct ActiveProcess<'a> {
    variant: &'a RecipeVariant,
    variable: Variable,
}

pub fn evaluate_population_from_crop_outputs(
    crop_outputs: &BTreeMap<String, f64>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<AllocationResult, AllocationError> {
    let prepared = prepare_allocation_problem(
        crop_outputs,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    )?;
    let (supported_population, process_runs, crop_inputs_used, crop_inputs_remaining, extra_outputs) =
        solve_allocation_two_stage(
            &prepared.available_crops,
            &prepared.food_requirement_rates,
            &prepared.recipe_requirements,
            &prepared.ordered_variants,
            filtered_foods,
        )?;

    let process_runs = prepared
        .ordered_variants
        .iter()
        .map(|variant| {
            (
                variant.name.clone(),
                process_runs.get(&variant.name).copied().unwrap_or(0.0),
            )
        })
        .filter(|(_, amount)| *amount > 1e-9)
        .collect::<BTreeMap<_, _>>();

    let food_outputs = build_food_outputs(filtered_foods, &prepared.ordered_variants, &process_runs);

    let chicken_summary = build_chicken_summary(&prepared.ordered_variants, &process_runs);

    Ok(AllocationResult {
        supported_population,
        food_outputs,
        extra_outputs,
        process_runs,
        crop_inputs_used,
        crop_inputs_remaining,
        chicken_summary,
    })
}

pub fn compare_allocation_stages_from_crop_outputs(
    crop_outputs: &BTreeMap<String, f64>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<AllocationStageComparison, AllocationError> {
    let prepared = prepare_allocation_problem(
        crop_outputs,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    )?;
    let phase1_tuple = solve_allocation_model(
        &prepared.available_crops,
        &prepared.food_requirement_rates,
        &prepared.recipe_requirements,
        &prepared.ordered_variants,
        None,
        AllocationObjective::Population,
        filtered_foods,
    )?;
    let stabilized_population = round_down_population_target(phase1_tuple.0);
    let phase2_tuple = solve_allocation_model(
        &prepared.available_crops,
        &prepared.food_requirement_rates,
        &prepared.recipe_requirements,
        &prepared.ordered_variants,
        Some(stabilized_population),
        AllocationObjective::SlackOutputs,
        filtered_foods,
    )?;
    let phase1_max_population = phase1_tuple.0;

    Ok(AllocationStageComparison {
        phase1: allocation_result_from_tuple(filtered_foods, &prepared.ordered_variants, phase1_tuple),
        phase2: allocation_result_from_tuple(filtered_foods, &prepared.ordered_variants, phase2_tuple),
        phase1_max_population,
        stabilized_population,
    })
}

fn allocation_result_from_tuple(
    filtered_foods: &[&str],
    ordered_variants: &[RecipeVariant],
    tuple: (
        f64,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
    ),
) -> AllocationResult {
    let (supported_population, process_runs, crop_inputs_used, crop_inputs_remaining, extra_outputs) = tuple;
    let process_runs = ordered_variants
        .iter()
        .map(|variant| {
            (
                variant.name.clone(),
                process_runs.get(&variant.name).copied().unwrap_or(0.0),
            )
        })
        .filter(|(_, amount)| *amount > 1e-9)
        .collect::<BTreeMap<_, _>>();

    let food_outputs = build_food_outputs(filtered_foods, ordered_variants, &process_runs);

    let chicken_summary = build_chicken_summary(ordered_variants, &process_runs);

    AllocationResult {
        supported_population,
        food_outputs,
        extra_outputs,
        process_runs,
        crop_inputs_used,
        crop_inputs_remaining,
        chicken_summary,
    }
}

fn build_chicken_summary(
    ordered_variants: &[RecipeVariant],
    process_runs: &BTreeMap<String, f64>,
) -> Option<ChickenSummary> {
    let mut animal_feed_sources = BTreeMap::<String, f64>::new();
    let mut full_farms_needed = 0.0;
    let mut chickens_needed = 0.0;
    let mut eggs_produced = 0.0;
    let mut carcasses_produced = 0.0;

    for variant in ordered_variants {
        let run_count = process_runs.get(&variant.name).copied().unwrap_or(0.0);
        if run_count <= 1e-9 {
            continue;
        }

        if let Some(source) = &variant.animal_feed_source {
            *animal_feed_sources.entry(source.clone()).or_insert(0.0) += run_count * variant.animal_feed_used;
        }
        full_farms_needed += run_count * variant.chicken_farm_runs;
        chickens_needed += run_count * variant.chicken_farm_runs * FULL_CHICKEN_FARM_SIZE;
        eggs_produced += run_count * variant.chicken_eggs_produced;
        carcasses_produced += run_count * variant.chicken_carcasses_produced;
    }

    (chickens_needed > 1e-9).then_some(ChickenSummary {
        animal_feed_sources,
        full_farms_needed,
        chickens_needed,
        eggs_produced,
        carcasses_produced,
    })
}

fn solve_allocation_two_stage(
    available_crops: &BTreeMap<String, f64>,
    food_requirement_rates: &BTreeMap<String, f64>,
    recipe_requirements: &BTreeMap<String, f64>,
    ordered_variants: &[RecipeVariant],
    filtered_foods: &[&str],
) -> Result<
    (
        f64,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
    ),
    AllocationError,
> {
    let max_population = solve_allocation_model(
        available_crops,
        food_requirement_rates,
        recipe_requirements,
        ordered_variants,
        None,
        AllocationObjective::Population,
        filtered_foods,
    )?
    .0;

    let stabilized_population = round_down_population_target(max_population);

    solve_allocation_model(
        available_crops,
        food_requirement_rates,
        recipe_requirements,
        ordered_variants,
        Some(stabilized_population),
        AllocationObjective::SlackOutputs,
        filtered_foods,
    )
}

#[derive(Clone, Copy)]
enum AllocationObjective {
    Population,
    SlackOutputs,
}

fn solve_allocation_model(
    available_crops: &BTreeMap<String, f64>,
    food_requirement_rates: &BTreeMap<String, f64>,
    recipe_requirements: &BTreeMap<String, f64>,
    ordered_variants: &[RecipeVariant],
    fixed_population: Option<f64>,
    objective_kind: AllocationObjective,
    filtered_foods: &[&str],
) -> Result<
    (
        f64,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
        BTreeMap<String, f64>,
    ),
    AllocationError,
> {
    let mut vars: ProblemVariables = variables!();
    let population_var = vars.add(variable().min(0.0));
    let mut active_processes = Vec::new();
    for variant in ordered_variants {
        let process_var = vars.add(variable().min(0.0));
        active_processes.push(ActiveProcess {
            variant,
            variable: process_var,
        });
    }

    let objective = match objective_kind {
        AllocationObjective::Population => population_var.into(),
        AllocationObjective::SlackOutputs => active_processes.iter().fold(Expression::from(0.0), |acc, process| {
            let compost = process.variant.outputs.get("Compost").copied().unwrap_or(0.0);
            let animal_feed = process.variant.outputs.get("Animal Feed").copied().unwrap_or(0.0);
            let other_extra = process
                .variant
                .outputs
                .iter()
                .filter(|(name, _)| !filtered_foods.contains(&name.as_str()))
                .filter(|(name, _)| name.as_str() != "Compost" && name.as_str() != "Animal Feed")
                .map(|(_, amount)| *amount)
                .sum::<f64>();
            acc + process.variable * (compost * 1_000.0 + animal_feed * 100.0 + other_extra)
        }),
    };

    let mut model = vars.maximise(objective).using(default_solver);

    for (crop_name, available_amount) in available_crops {
        let usage_expr = active_processes.iter().fold(Expression::from(0.0), |acc, process| {
            acc + process.variable * process.variant.inputs.get(crop_name).copied().unwrap_or(0.0)
        });
        model = model.with(usage_expr.leq(*available_amount));
    }

    for (food, rate) in food_requirement_rates {
        let output_expr = active_processes.iter().fold(Expression::from(0.0), |acc, process| {
            acc + process.variable * process.variant.outputs.get(food).copied().unwrap_or(0.0)
        });
        model = model.with(output_expr.geq(population_var * *rate));
    }

    for (output, requirement) in recipe_requirements {
        let output_expr = active_processes.iter().fold(Expression::from(0.0), |acc, process| {
            acc + process.variable * process.variant.outputs.get(output).copied().unwrap_or(0.0)
        });
        model = model.with(output_expr.geq(*requirement));
    }

    if let Some(value) = fixed_population {
        model = model.with(Expression::from(population_var).eq(value));
    }

    let solution = model.solve().map_err(|error| AllocationError::Solver(error.to_string()))?;
    let supported_population = solution.value(population_var);
    let process_runs = active_processes
        .iter()
        .map(|process| (process.variant.name.clone(), solution.value(process.variable)))
        .collect::<BTreeMap<_, _>>();
    let crop_inputs_used = available_crops
        .keys()
        .map(|crop_name| {
            let amount = active_processes.iter().fold(0.0, |acc, process| {
                acc + solution.value(process.variable)
                    * process.variant.inputs.get(crop_name).copied().unwrap_or(0.0)
            });
            (crop_name.clone(), amount)
        })
        .collect::<BTreeMap<_, _>>();
    let crop_inputs_remaining = available_crops
        .iter()
        .map(|(crop_name, available_amount)| {
            let used_amount = crop_inputs_used.get(crop_name).copied().unwrap_or(0.0);
            (crop_name.clone(), (available_amount - used_amount).max(0.0))
        })
        .collect::<BTreeMap<_, _>>();
    let extra_outputs = active_processes
        .iter()
        .fold(BTreeMap::<String, f64>::new(), |mut acc, process| {
            let run_count = solution.value(process.variable);
            for (name, amount) in &process.variant.outputs {
                if filtered_foods.contains(&name.as_str()) {
                    continue;
                }
                *acc.entry(name.clone()).or_insert(0.0) += run_count * amount;
            }
            acc
        });

    Ok((
        supported_population,
        process_runs,
        crop_inputs_used,
        crop_inputs_remaining,
        extra_outputs,
    ))
}

fn round_down_population_target(value: f64) -> f64 {
    if value >= 100.0 {
        (value / 100.0).floor() * 100.0
    } else {
        value.floor().max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::io::recipe::AuthoritativeRecipeData;

    use super::evaluate_population_from_crop_outputs;

    fn recipe_data() -> AuthoritativeRecipeData {
        AuthoritativeRecipeData::load(&PathBuf::from("captain-of-data/data/machines_and_buildings.json"))
            .expect("recipe data should load")
    }

    #[test]
    fn direct_foods_and_tofu_support_expected_population() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Potatoes".to_owned(), 14.0),
                ("Corn".to_owned(), 10.0),
                ("Vegetables".to_owned(), 28.0),
                ("Soybean".to_owned(), 9.0),
            ]),
            &["Potatoes", "Corn", "Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");

        assert!((result.supported_population - 2000.0).abs() < 1e-4);
        assert!((result.food_outputs["Tofu"] - 12.0).abs() < 1e-6);
    }

    #[test]
    fn recipe_requirements_consume_crop_capacity() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let no_requirement = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Soybean".to_owned(), 9.0),
                ("Vegetables".to_owned(), 28.0),
            ]),
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");
        let with_food_pack = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Soybean".to_owned(), 9.0),
                ("Vegetables".to_owned(), 28.0),
            ]),
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::from([("Food Pack".to_owned(), 4.0)]),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");

        assert!(with_food_pack.supported_population < no_requirement.supported_population);
        assert!(with_food_pack.extra_outputs["Food Pack"] >= 4.0 - 1e-6);
    }

    #[test]
    fn direct_crop_requirements_make_infeasible_baskets_fail() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = evaluate_population_from_crop_outputs(
            &BTreeMap::from([("Potatoes".to_owned(), 5.0)]),
            &["Potatoes"],
            1.0,
            &BTreeMap::from([("Potatoes".to_owned(), 6.0)]),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        );

        assert!(result.is_err());
    }

    #[test]
    fn slack_food_is_processed_into_compost_or_animal_feed() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Potatoes".to_owned(), 10.0),
                ("Vegetables".to_owned(), 10.0),
                ("Corn".to_owned(), 20.0),
            ]),
            &["Potatoes"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");

        assert!(result.extra_outputs.get("Compost").copied().unwrap_or(0.0) > 0.0);
        assert!(result.extra_outputs.get("Animal Feed").copied().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn second_stage_population_is_stabilized_to_rounded_hundred() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Potatoes".to_owned(), 14.0),
                ("Corn".to_owned(), 10.0),
                ("Vegetables".to_owned(), 28.0),
                ("Soybean".to_owned(), 9.0),
            ]),
            &["Potatoes", "Corn", "Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");

        assert_eq!(result.supported_population, 2000.0);
    }

    #[test]
    fn unrequested_requirement_variants_are_not_emitted_as_slack_outputs() {
        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = evaluate_population_from_crop_outputs(
            &BTreeMap::from([
                ("Canola".to_owned(), 50.0),
                ("Corn".to_owned(), 50.0),
                ("Potatoes".to_owned(), 50.0),
                ("Soybean".to_owned(), 50.0),
                ("Sugar Cane".to_owned(), 50.0),
                ("Vegetables".to_owned(), 50.0),
                ("Wheat".to_owned(), 50.0),
            ]),
            &["Potatoes"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("allocation should succeed");

        assert_eq!(result.extra_outputs.get("Cooking Oil").copied().unwrap_or(0.0), 0.0);
        assert_eq!(result.extra_outputs.get("Food Pack").copied().unwrap_or(0.0), 0.0);
        assert_eq!(result.extra_outputs.get("Sugar").copied().unwrap_or(0.0), 0.0);
        assert_eq!(result.extra_outputs.get("Ethanol").copied().unwrap_or(0.0), 0.0);
    }
}
