use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use good_lp::{variable, variables, Expression, ProblemVariables, SolverModel, Variable};
use good_lp::solvers::highs::highs as highs_solver;
use highs::{HighsModelStatus, HighsSolutionStatus};
use highs_sys::{
    HighsCallbackDataIn, HighsCallbackDataOut, Highs_setCallback, Highs_startCallback,
    STATUS_OK, kHighsCallbackMipImprovingSolution, kHighsCallbackMipInterrupt,
    kHighsCallbackMipLogging,
};

use crate::domain::allocation_problem::{
    exact_food_requirement_rates, producible_crops_by_name, split_requirements,
};
use crate::domain::allocation::{evaluate_population_from_crop_outputs, AllocationError, AllocationResult};
use crate::domain::catalog::CatalogOption;
use crate::domain::crop::BuildingType;
use crate::domain::recipe::RecipeVariant;

const POPULATION_OBJECTIVE_WEIGHT: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SolverProgress {
    pub running_time_seconds: f64,
    pub best_objective_value: f64,
    pub best_population_estimate: f64,
    pub dual_bound: f64,
    pub mip_gap: f64,
    pub mip_node_count: i64,
}

pub type ProgressCallback = Arc<dyn Fn(SolverProgress) + Send + Sync + 'static>;

struct ProgressCallbackContext {
    callback: ProgressCallback,
    cancel_requested: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedOption {
    pub count: u32,
    pub option: CatalogOption,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationResult {
    pub phase1_interrupted: bool,
    pub supported_population: f64,
    pub total_fertilizer_per_month: f64,
    pub total_water_per_month: f64,
    pub crop_outputs: BTreeMap<String, f64>,
    pub allocation: AllocationResult,
    pub selected_options: Vec<SelectedOption>,
    pub selected_options_by_building: BTreeMap<BuildingType, Vec<SelectedOption>>,
}

#[derive(Debug)]
pub enum OptimizerError {
    Allocation(AllocationError),
    NoFeasibleSolution,
}

impl Display for OptimizerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allocation(error) => write!(f, "{error}"),
            Self::NoFeasibleSolution => write!(f, "no feasible solution found"),
        }
    }
}

impl std::error::Error for OptimizerError {}

impl From<AllocationError> for OptimizerError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

#[derive(Debug, Clone)]
struct CandidateEvaluation {
    supported_population: f64,
    total_fertilizer_per_month: f64,
    total_water_per_month: f64,
    crop_outputs: BTreeMap<String, f64>,
    allocation: AllocationResult,
    selected_options: Vec<SelectedOption>,
    selected_options_by_building: BTreeMap<BuildingType, Vec<SelectedOption>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildingPool {
    pub farm_count: u32,
    pub options: Vec<CatalogOption>,
}

pub fn optimize_count_mix(
    farm_count: u32,
    options: &[CatalogOption],
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<OptimizationResult, OptimizerError> {
    let mut counts = vec![0_u32; options.len()];
    let mut best: Option<CandidateEvaluation> = None;

    enumerate_count_vectors(
        options,
        0,
        farm_count,
        &mut counts,
        &mut |candidate_counts| {
            if let Ok(candidate) = evaluate_candidate(
                options,
                candidate_counts,
                filtered_foods,
                food_multiplier,
                extra_requirements,
                food_variants,
                recipe_requirement_variants,
                slack_sink_variants,
            ) {
                let replace = best
                    .as_ref()
                    .map(|current| is_better_candidate(&candidate, current))
                    .unwrap_or(true);
                if replace {
                    best = Some(candidate);
                }
            }
        },
    );

    let best = best.ok_or(OptimizerError::NoFeasibleSolution)?;
    Ok(OptimizationResult {
        phase1_interrupted: false,
        supported_population: best.supported_population,
        total_fertilizer_per_month: best.total_fertilizer_per_month,
        total_water_per_month: best.total_water_per_month,
        crop_outputs: best.crop_outputs,
        allocation: best.allocation,
        selected_options: best.selected_options,
        selected_options_by_building: best.selected_options_by_building,
    })
}

pub fn optimize_building_mix(
    building_pools: &BTreeMap<BuildingType, BuildingPool>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<OptimizationResult, OptimizerError> {
    let mut building_entries = building_pools
        .iter()
        .filter(|(_, pool)| pool.farm_count > 0 && !pool.options.is_empty())
        .map(|(building, pool)| (*building, pool))
        .collect::<Vec<_>>();
    building_entries.sort_by_key(|(building, _)| *building);

    let mut counts_by_building = building_entries
        .iter()
        .map(|(_, pool)| vec![0_u32; pool.options.len()])
        .collect::<Vec<_>>();
    let mut best: Option<CandidateEvaluation> = None;

    enumerate_building_count_vectors(
        &building_entries,
        0,
        &mut counts_by_building,
        &mut |candidate_counts| {
            if let Ok(candidate) = evaluate_building_candidate(
                &building_entries,
                candidate_counts,
                filtered_foods,
                food_multiplier,
                extra_requirements,
                food_variants,
                recipe_requirement_variants,
                slack_sink_variants,
            ) {
                let replace = best
                    .as_ref()
                    .map(|current| is_better_candidate(&candidate, current))
                    .unwrap_or(true);
                if replace {
                    best = Some(candidate);
                }
            }
        },
    );

    let best = best.ok_or(OptimizerError::NoFeasibleSolution)?;
    Ok(OptimizationResult {
        phase1_interrupted: false,
        supported_population: best.supported_population,
        total_fertilizer_per_month: best.total_fertilizer_per_month,
        total_water_per_month: best.total_water_per_month,
        crop_outputs: best.crop_outputs,
        allocation: best.allocation,
        selected_options: best.selected_options,
        selected_options_by_building: best.selected_options_by_building,
    })
}

pub fn optimize_building_mix_mip(
    building_pools: &BTreeMap<BuildingType, BuildingPool>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<OptimizationResult, OptimizerError> {
    optimize_building_mix_mip_with_progress(
        building_pools,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
        None,
        None,
    )
}

pub fn optimize_building_mix_mip_with_progress(
    building_pools: &BTreeMap<BuildingType, BuildingPool>,
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
    progress_callback: Option<ProgressCallback>,
    cancel_requested: Option<Arc<AtomicBool>>,
) -> Result<OptimizationResult, OptimizerError> {
    let mut building_entries = building_pools
        .iter()
        .filter(|(_, pool)| pool.farm_count > 0 && !pool.options.is_empty())
        .map(|(building, pool)| (*building, pool))
        .collect::<Vec<_>>();
    building_entries.sort_by_key(|(building, _)| *building);

    let producible_crops = producible_crops_by_name(building_entries.iter().flat_map(|(_, pool)| {
        pool.options.iter().flat_map(|option| {
            option
                .simulation
                .individual_effective_yields
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        })
    }));
    let (direct_crop_requirements, recipe_requirements) =
        split_requirements(extra_requirements, recipe_requirement_variants);
    let food_requirement_rates =
        exact_food_requirement_rates(filtered_foods, food_multiplier).map_err(OptimizerError::Allocation)?;

    let active_food_variants = filtered_foods
        .iter()
        .flat_map(|food| {
            food_variants
                .get(*food)
                .into_iter()
                .flat_map(|variants| variants.iter())
                .filter(|variant| variant.inputs.keys().all(|crop| producible_crops.contains(crop)))
                .cloned()
        })
        .collect::<Vec<_>>();

    let active_recipe_variants = recipe_requirements
        .keys()
        .filter_map(|output| recipe_requirement_variants.get(output))
        .flat_map(|variants| variants.iter())
        .filter(|variant| variant.inputs.keys().all(|crop| producible_crops.contains(crop)))
        .cloned()
        .collect::<Vec<_>>();

    let mut unique_process_variants = BTreeMap::<String, RecipeVariant>::new();
    for variant in active_food_variants.into_iter().chain(active_recipe_variants.into_iter()) {
        unique_process_variants
            .entry(variant.name.clone())
            .or_insert(variant);
    }

    let mut vars: ProblemVariables = variables!();
    let population_var = vars.add(variable().min(0.0));

    let mut next_column_index = 1_usize;
    let mut option_vars = Vec::<(BuildingType, &CatalogOption, Variable, usize)>::new();
    for (building, pool) in &building_entries {
        for option in &pool.options {
            let count_var = vars.add(
                variable()
                    .integer()
                    .min(0.0)
                    .max(pool.farm_count as f64),
            );
            option_vars.push((*building, option, count_var, next_column_index));
            next_column_index += 1;
        }
    }

    let mut process_vars = Vec::<(&RecipeVariant, Variable)>::new();
    for variant in unique_process_variants.values() {
        let process_var = vars.add(variable().min(0.0));
        process_vars.push((variant, process_var));
    }

    let objective = population_var * POPULATION_OBJECTIVE_WEIGHT
        - option_vars.iter().fold(Expression::from(0.0), |acc, (_, option, var, _)| {
            acc + *var * (2.0 * option.simulation.fertilizer_required_per_month + option.simulation.water_per_month)
        });
    let mut model = vars.maximise(objective).using(highs_solver);

    for (building, pool) in &building_entries {
        let building_expr = option_vars.iter().fold(Expression::from(0.0), |acc, (option_building, _, var, _)| {
            if option_building == building {
                acc + *var
            } else {
                acc
            }
        });
        model = model.with(building_expr.eq(pool.farm_count as f64));
    }

    for crop_name in producible_crops.iter() {
        let produced_expr = option_vars.iter().fold(Expression::from(0.0), |acc, (_, option, var, _)| {
            acc + *var * option.simulation.individual_effective_yields.get(crop_name).copied().unwrap_or(0.0)
        });
        let used_expr = process_vars.iter().fold(Expression::from(0.0), |acc, (variant, var)| {
            acc + *var * variant.inputs.get(crop_name).copied().unwrap_or(0.0)
        });
        let required = direct_crop_requirements.get(crop_name).copied().unwrap_or(0.0);
        model = model.with((used_expr + required).leq(produced_expr));
    }

    for (food, rate) in &food_requirement_rates {
        let output_expr = process_vars.iter().fold(Expression::from(0.0), |acc, (variant, var)| {
            acc + *var * variant.outputs.get(food).copied().unwrap_or(0.0)
        });
        model = model.with(output_expr.geq(population_var * *rate));
    }

    for (output, requirement) in &recipe_requirements {
        let output_expr = process_vars.iter().fold(Expression::from(0.0), |acc, (variant, var)| {
            acc + *var * variant.outputs.get(output).copied().unwrap_or(0.0)
        });
        model = model.with(output_expr.geq(*requirement));
    }

    let mut highs_model = model.into_inner();
    highs_model.set_option("parallel", "on");
    highs_model.set_option("threads", 0);
    highs_model.set_option("mip_min_logging_interval", 2.0);

    let cancel_requested = cancel_requested.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let mut callback_context = progress_callback.map(|callback| {
        Box::new(ProgressCallbackContext {
            callback,
            cancel_requested: cancel_requested.clone(),
        })
    });

    if let Some(context) = callback_context.as_mut() {
        let status = unsafe {
            Highs_setCallback(
                highs_model.as_mut_ptr(),
                Some(highs_progress_callback),
                (&mut **context as *mut ProgressCallbackContext).cast(),
            )
        };
        if status == STATUS_OK {
            unsafe {
                Highs_startCallback(highs_model.as_mut_ptr(), kHighsCallbackMipLogging as _);
                Highs_startCallback(
                    highs_model.as_mut_ptr(),
                    kHighsCallbackMipImprovingSolution as _,
                );
                Highs_startCallback(highs_model.as_mut_ptr(), kHighsCallbackMipInterrupt as _);
            }
        }
    }

    let solved = highs_model
        .try_solve()
        .map_err(|error| OptimizerError::Allocation(AllocationError::Solver(format!("{error:?}"))))?;

    let status = solved.status();
    let phase1_interrupted = matches!(status, HighsModelStatus::ReachedInterrupt);
    match status {
        HighsModelStatus::Infeasible
        | HighsModelStatus::Unbounded
        | HighsModelStatus::UnboundedOrInfeasible => {
            return Err(OptimizerError::Allocation(AllocationError::Infeasible));
        }
        HighsModelStatus::NotSet
        | HighsModelStatus::LoadError
        | HighsModelStatus::ModelError
        | HighsModelStatus::PresolveError
        | HighsModelStatus::SolveError
        | HighsModelStatus::PostsolveError
        | HighsModelStatus::ModelEmpty => {
            return Err(OptimizerError::Allocation(AllocationError::Solver(format!(
                "{status:?}"
            ))));
        }
        _ => {}
    }

    if solved.primal_solution_status() != HighsSolutionStatus::Feasible {
        return Err(OptimizerError::Allocation(AllocationError::Solver(
            "NoSolutionFound".to_owned(),
        )));
    }

    let solution = solved.get_solution();

    let mut crop_outputs = BTreeMap::<String, f64>::new();
    let mut total_fertilizer_per_month = 0.0;
    let mut total_water_per_month = 0.0;
    let mut selected_options = Vec::new();
    let mut selected_options_by_building = BTreeMap::<BuildingType, Vec<SelectedOption>>::new();

    for (building, option, _var, column_index) in &option_vars {
        let count = solution.columns()[*column_index].round() as u32;
        if count == 0 {
            continue;
        }
        let count_f = count as f64;
        total_fertilizer_per_month += option.simulation.fertilizer_required_per_month * count_f;
        total_water_per_month += option.simulation.water_per_month * count_f;
        for (crop_name, amount) in &option.simulation.individual_effective_yields {
            *crop_outputs.entry(crop_name.clone()).or_insert(0.0) += amount * count_f;
        }
        let selected = SelectedOption {
            count,
            option: (*option).clone(),
        };
        selected_options.push(selected.clone());
        selected_options_by_building
            .entry(*building)
            .or_default()
            .push(selected);
    }

    let allocation = evaluate_population_from_crop_outputs(
        &crop_outputs,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    )?;

    Ok(OptimizationResult {
        phase1_interrupted,
        supported_population: allocation.supported_population,
        total_fertilizer_per_month,
        total_water_per_month,
        crop_outputs,
        allocation,
        selected_options,
        selected_options_by_building,
    })
}

fn enumerate_count_vectors(
    options: &[CatalogOption],
    index: usize,
    remaining: u32,
    counts: &mut [u32],
    visit: &mut impl FnMut(&[u32]),
) {
    if index == options.len() {
        if remaining == 0 {
            visit(counts);
        }
        return;
    }

    if index == options.len() - 1 {
        counts[index] = remaining;
        visit(counts);
        return;
    }

    for count in 0..=remaining {
        counts[index] = count;
        enumerate_count_vectors(options, index + 1, remaining - count, counts, visit);
    }
}

fn evaluate_candidate(
    options: &[CatalogOption],
    counts: &[u32],
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<CandidateEvaluation, OptimizerError> {
    let mut crop_outputs = BTreeMap::<String, f64>::new();
    let mut total_fertilizer_per_month = 0.0;
    let mut total_water_per_month = 0.0;
    let mut selected_options = Vec::new();

    for (option, count) in options.iter().zip(counts.iter().copied()) {
        if count == 0 {
            continue;
        }

        let count_f = count as f64;
        total_fertilizer_per_month += option.simulation.fertilizer_required_per_month * count_f;
        total_water_per_month += option.simulation.water_per_month * count_f;
        for (crop_name, amount) in &option.simulation.individual_effective_yields {
            *crop_outputs.entry(crop_name.clone()).or_insert(0.0) += amount * count_f;
        }
        selected_options.push(SelectedOption {
            count,
            option: option.clone(),
        });
    }

    let allocation = evaluate_population_from_crop_outputs(
        &crop_outputs,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    )?;

    Ok(CandidateEvaluation {
        supported_population: allocation.supported_population,
        total_fertilizer_per_month,
        total_water_per_month,
        crop_outputs,
        allocation,
        selected_options,
        selected_options_by_building: BTreeMap::new(),
    })
}

fn enumerate_building_count_vectors<'a>(
    building_entries: &[(BuildingType, &'a BuildingPool)],
    building_index: usize,
    counts_by_building: &mut [Vec<u32>],
    visit: &mut impl FnMut(&[Vec<u32>]),
) {
    if building_index == building_entries.len() {
        visit(counts_by_building);
        return;
    }

    let (_, pool) = building_entries[building_index];
    let mut building_counts = vec![0_u32; pool.options.len()];
    enumerate_single_building_counts(
        pool.options.len(),
        0,
        pool.farm_count,
        &mut building_counts,
        &mut |counts| {
            counts_by_building[building_index].copy_from_slice(counts);
            enumerate_building_count_vectors(building_entries, building_index + 1, counts_by_building, visit);
        },
    );
}

fn enumerate_single_building_counts(
    option_count: usize,
    index: usize,
    remaining: u32,
    counts: &mut [u32],
    visit: &mut impl FnMut(&[u32]),
) {
    if index == option_count {
        if remaining == 0 {
            visit(counts);
        }
        return;
    }

    if index == option_count - 1 {
        counts[index] = remaining;
        visit(counts);
        return;
    }

    for count in 0..=remaining {
        counts[index] = count;
        enumerate_single_building_counts(option_count, index + 1, remaining - count, counts, visit);
    }
}

fn evaluate_building_candidate(
    building_entries: &[(BuildingType, &BuildingPool)],
    counts_by_building: &[Vec<u32>],
    filtered_foods: &[&str],
    food_multiplier: f64,
    extra_requirements: &BTreeMap<String, f64>,
    food_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    recipe_requirement_variants: &BTreeMap<String, Vec<RecipeVariant>>,
    slack_sink_variants: &[RecipeVariant],
) -> Result<CandidateEvaluation, OptimizerError> {
    let mut crop_outputs = BTreeMap::<String, f64>::new();
    let mut total_fertilizer_per_month = 0.0;
    let mut total_water_per_month = 0.0;
    let mut selected_options = Vec::new();
    let mut selected_options_by_building = BTreeMap::<BuildingType, Vec<SelectedOption>>::new();

    for ((building, pool), counts) in building_entries.iter().zip(counts_by_building) {
        for (option, count) in pool.options.iter().zip(counts.iter().copied()) {
            if count == 0 {
                continue;
            }

            let count_f = count as f64;
            total_fertilizer_per_month += option.simulation.fertilizer_required_per_month * count_f;
            total_water_per_month += option.simulation.water_per_month * count_f;
            for (crop_name, amount) in &option.simulation.individual_effective_yields {
                *crop_outputs.entry(crop_name.clone()).or_insert(0.0) += amount * count_f;
            }

            let selected = SelectedOption {
                count,
                option: option.clone(),
            };
            selected_options.push(selected.clone());
            selected_options_by_building
                .entry(*building)
                .or_default()
                .push(selected);
        }
    }

    let allocation = evaluate_population_from_crop_outputs(
        &crop_outputs,
        filtered_foods,
        food_multiplier,
        extra_requirements,
        food_variants,
        recipe_requirement_variants,
        slack_sink_variants,
    )?;

    Ok(CandidateEvaluation {
        supported_population: allocation.supported_population,
        total_fertilizer_per_month,
        total_water_per_month,
        crop_outputs,
        allocation,
        selected_options,
        selected_options_by_building,
    })
}

fn is_better_candidate(candidate: &CandidateEvaluation, current: &CandidateEvaluation) -> bool {
    const POP_EPS: f64 = 1e-6;
    const COST_EPS: f64 = 1e-6;

    if candidate.supported_population > current.supported_population + POP_EPS {
        return true;
    }
    if candidate.supported_population + POP_EPS < current.supported_population {
        return false;
    }

    let candidate_cost = 2.0 * candidate.total_fertilizer_per_month + candidate.total_water_per_month;
    let current_cost = 2.0 * current.total_fertilizer_per_month + current.total_water_per_month;
    if candidate_cost + COST_EPS < current_cost {
        return true;
    }
    if current_cost + COST_EPS < candidate_cost {
        return false;
    }

    let candidate_rotation_count = candidate.selected_options.len();
    let current_rotation_count = current.selected_options.len();
    candidate_rotation_count < current_rotation_count
}

unsafe extern "C" fn highs_progress_callback(
    _callback_type: i32,
    _message: *const std::os::raw::c_char,
    data_out: *const HighsCallbackDataOut,
    data_in: *mut HighsCallbackDataIn,
    user_callback_data: *mut std::ffi::c_void,
) {
    if data_out.is_null() || user_callback_data.is_null() {
        return;
    }

    let data = unsafe { &*data_out };
    let context = unsafe { &*(user_callback_data as *const ProgressCallbackContext) };

    let best_objective_value = data.mip_primal_bound;
    let best_population_estimate = best_objective_value / POPULATION_OBJECTIVE_WEIGHT;
    (context.callback)(SolverProgress {
        running_time_seconds: data.running_time,
        best_objective_value,
        best_population_estimate,
        dual_bound: data.mip_dual_bound,
        mip_gap: data.mip_gap,
        mip_node_count: data.mip_node_count,
    });

    if !data_in.is_null() && context.cancel_requested.load(Ordering::Relaxed) {
        unsafe {
            (*data_in).user_interrupt = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::domain::catalog::{build_baseline_catalog_by_building, build_option_catalog};
    use crate::domain::crop::BuildingType;
    use crate::domain::fertility::FERTILIZER_ORGANIC;
    use crate::io::recipe::AuthoritativeRecipeData;
    use crate::io::wiki::load_crop_catalog;

    use super::{optimize_building_mix, optimize_building_mix_mip, optimize_count_mix, BuildingPool};

    fn crop_catalog() -> crate::domain::crop::CropCatalog {
        load_crop_catalog(&PathBuf::from("data/wiki_crop_data.json")).expect("catalog should load")
    }

    fn recipe_data() -> AuthoritativeRecipeData {
        AuthoritativeRecipeData::load(&PathBuf::from("captain-of-data/data/machines_and_buildings.json"))
            .expect("recipe data should load")
    }

    #[test]
    fn optimizer_beats_or_matches_every_single_option_for_one_farm() {
        let catalog = crop_catalog();
        let options = build_option_catalog(
            &catalog,
            &["Soybean".to_owned(), "Vegetables".to_owned()],
            BuildingType::FarmT2,
            &[Some(100.0)],
            Some(FERTILIZER_ORGANIC),
        )
        .expect("options should build");

        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let best = optimize_count_mix(
            1,
            &options,
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("optimizer should find a solution");

        for option in &options {
            let single = optimize_count_mix(
                1,
                std::slice::from_ref(option),
                &["Vegetables", "Tofu"],
                1.0,
                &BTreeMap::new(),
                &food_variants,
                &recipe_variants,
                &slack_variants,
            )
            .expect("single-option optimization should work");
            assert!(best.supported_population + 1e-6 >= single.supported_population);
        }
    }

    #[test]
    fn optimizer_uses_mixed_rotation_when_it_strictly_dominates_split_for_same_output_goal() {
        let catalog = crop_catalog();
        let all_options = build_option_catalog(
            &catalog,
            &["Soybean".to_owned(), "Vegetables".to_owned()],
            BuildingType::FarmT2,
            &[Some(100.0)],
            Some(FERTILIZER_ORGANIC),
        )
        .expect("options should build");

        let options = all_options
            .into_iter()
            .filter(|option| {
                let name = option.rotation.0.join(" -> ");
                name == "Soybean"
                    || name == "Vegetables"
                    || name == "Soybean -> Vegetables"
            })
            .collect::<Vec<_>>();

        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let best = optimize_count_mix(
            2,
            &options,
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("optimizer should find a solution");

        assert_eq!(best.selected_options.len(), 1);
        assert_eq!(best.selected_options[0].count, 2);
        assert_eq!(
            best.selected_options[0].option.rotation.0,
            vec!["Soybean".to_owned(), "Vegetables".to_owned()]
        );
    }

    #[test]
    fn building_optimizer_tracks_selections_per_building_type() {
        let catalog = crop_catalog();
        let baseline_catalog = build_baseline_catalog_by_building(
            &catalog,
            &["Soybean".to_owned(), "Vegetables".to_owned(), "Fruit".to_owned()],
            &BTreeMap::from([
                (BuildingType::FarmT2, Some(100.0)),
                (BuildingType::FarmT3, Some(100.0)),
            ]),
            Some(FERTILIZER_ORGANIC),
        )
        .expect("baseline catalog should build");

        let farm_t2_options = baseline_catalog[&BuildingType::FarmT2]
            .iter()
            .filter(|option| option.rotation.0 == vec!["Soybean".to_owned(), "Vegetables".to_owned()])
            .cloned()
            .collect::<Vec<_>>();
        let farm_t3_options = baseline_catalog[&BuildingType::FarmT3]
            .iter()
            .filter(|option| {
                option.rotation.0 == vec!["Fruit".to_owned()]
                    || option.rotation.0 == vec!["Vegetables".to_owned()]
            })
            .cloned()
            .collect::<Vec<_>>();

        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let result = optimize_building_mix(
            &BTreeMap::from([
                (
                    BuildingType::FarmT2,
                    BuildingPool {
                        farm_count: 1,
                        options: farm_t2_options,
                    },
                ),
                (
                    BuildingType::FarmT3,
                    BuildingPool {
                        farm_count: 1,
                        options: farm_t3_options,
                    },
                ),
            ]),
            &["Vegetables", "Tofu", "Fruit"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("optimizer should find a solution");

        assert_eq!(
            result.selected_options_by_building[&BuildingType::FarmT2]
                .iter()
                .map(|selection| selection.count)
                .sum::<u32>(),
            1
        );
        assert_eq!(
            result.selected_options_by_building[&BuildingType::FarmT3]
                .iter()
                .map(|selection| selection.count)
                .sum::<u32>(),
            1
        );
        assert!(result.supported_population > 0.0);
    }

    #[test]
    fn mip_optimizer_matches_exhaustive_optimizer_on_small_case() {
        let catalog = crop_catalog();
        let baseline_catalog = build_baseline_catalog_by_building(
            &catalog,
            &["Soybean".to_owned(), "Vegetables".to_owned()],
            &BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
            Some(FERTILIZER_ORGANIC),
        )
        .expect("baseline catalog should build");

        let options = baseline_catalog[&BuildingType::FarmT2]
            .iter()
            .filter(|option| {
                option.rotation.0 == vec!["Soybean".to_owned()]
                    || option.rotation.0 == vec!["Vegetables".to_owned()]
                    || option.rotation.0 == vec!["Soybean".to_owned(), "Vegetables".to_owned()]
            })
            .cloned()
            .collect::<Vec<_>>();

        let pools = BTreeMap::from([(
            BuildingType::FarmT2,
            BuildingPool {
                farm_count: 2,
                options,
            },
        )]);

        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let exhaustive = optimize_building_mix(
            &pools,
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("exhaustive optimizer should work");
        let mip = optimize_building_mix_mip(
            &pools,
            &["Vegetables", "Tofu"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("mip optimizer should work");

        assert!((mip.supported_population - exhaustive.supported_population).abs() < 1e-6);
        assert!((mip.total_fertilizer_per_month - exhaustive.total_fertilizer_per_month).abs() < 1e-6);
        assert!((mip.total_water_per_month - exhaustive.total_water_per_month).abs() < 1e-6);
        assert_eq!(mip.selected_options_by_building, exhaustive.selected_options_by_building);
    }

    #[test]
    fn corn_and_potatoes_choose_monocultures_because_mixed_rotation_loses_population() {
        let catalog = crop_catalog();
        let baseline_catalog = build_baseline_catalog_by_building(
            &catalog,
            &["Corn".to_owned(), "Potatoes".to_owned()],
            &BTreeMap::from([(BuildingType::FarmT2, Some(100.0))]),
            Some(FERTILIZER_ORGANIC),
        )
        .expect("baseline catalog should build");

        let options = baseline_catalog[&BuildingType::FarmT2]
            .iter()
            .filter(|option| {
                option.rotation.0 == vec!["Corn".to_owned()]
                    || option.rotation.0 == vec!["Potatoes".to_owned()]
                    || option.rotation.0 == vec!["Corn".to_owned(), "Potatoes".to_owned()]
            })
            .cloned()
            .collect::<Vec<_>>();

        let pools = BTreeMap::from([(
            BuildingType::FarmT2,
            BuildingPool {
                farm_count: 2,
                options: options.clone(),
            },
        )]);

        let data = recipe_data();
        let food_variants = data.build_food_variants().expect("food variants should build");
        let recipe_variants = data
            .build_requirement_variants()
            .expect("requirement variants should build");
        let slack_variants = data
            .build_slack_sink_variants()
            .expect("slack variants should build");

        let best = optimize_building_mix_mip(
            &pools,
            &["Corn", "Potatoes"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("mip optimizer should work");

        let mixed_only = optimize_count_mix(
            2,
            &[options
                .iter()
                .find(|option| option.rotation.0 == vec!["Corn".to_owned(), "Potatoes".to_owned()])
                .expect("mixed rotation should exist")
                .clone()],
            &["Corn", "Potatoes"],
            1.0,
            &BTreeMap::new(),
            &food_variants,
            &recipe_variants,
            &slack_variants,
        )
        .expect("mixed-only optimization should work");

        assert!(
            best.supported_population > mixed_only.supported_population + 1e-6,
            "expected monocultures to win on population before fertilizer tie-break"
        );
        assert_eq!(best.selected_options.len(), 2);
        assert!(best
            .selected_options
            .iter()
            .any(|selection| selection.option.rotation.0 == vec!["Corn".to_owned()]));
        assert!(best
            .selected_options
            .iter()
            .any(|selection| selection.option.rotation.0 == vec!["Potatoes".to_owned()]));
    }
}
