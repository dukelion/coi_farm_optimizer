use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use crate::domain::crop::{BuildingType, CropCatalog, CropDefinition};

const NATURAL_REPLENISHMENT_PER_SECOND_PER_PERCENT: f64 = 0.01;
const MONTH_SECONDS: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FertilizerProduct {
    pub name: &'static str,
    pub fertility_per_quantity_percent: f64,
    pub max_fertility_percent: f64,
}

pub const FERTILIZER_ORGANIC: FertilizerProduct = FertilizerProduct {
    name: "Fertilizer (organic)",
    fertility_per_quantity_percent: 1.0,
    max_fertility_percent: 100.0,
};

pub const FERTILIZER_I: FertilizerProduct = FertilizerProduct {
    name: "Fertilizer I",
    fertility_per_quantity_percent: 2.0,
    max_fertility_percent: 120.0,
};

pub const FERTILIZER_II: FertilizerProduct = FertilizerProduct {
    name: "Fertilizer II",
    fertility_per_quantity_percent: 2.5,
    max_fertility_percent: 140.0,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RotationSummary {
    pub total_duration_seconds: u32,
    pub natural_equilibrium_percent: f64,
    pub average_positive_drain_per_second: f64,
    pub water_per_month: f64,
    pub base_monthly_yields_at_100: BTreeMap<String, f64>,
    pub effective_yield_per_month_at_100: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FertilizerRequirement {
    pub fertilizer_name: Option<&'static str>,
    pub fertilizer_quantity_per_second: f64,
    pub fertilizer_required_per_month: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationResult {
    pub total_duration_seconds: u32,
    pub fertility_equilibrium: f64,
    pub average_actual_fertility: f64,
    pub fertilizer_required_per_month: f64,
    pub water_per_month: f64,
    pub yield_per_month_raw: f64,
    pub effective_yield_per_month: f64,
    pub individual_effective_yields: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FertilityError {
    UnknownCrop(String),
    UnsupportedCrop { crop: String, building: BuildingType },
    GreenhouseRequired(String),
    FertilizerTooWeak { fertilizer: &'static str, target_percent: f64 },
}

impl Display for FertilityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCrop(crop) => write!(f, "unknown crop: {crop}"),
            Self::UnsupportedCrop { crop, building } => {
                write!(f, "{crop} has no wiki data for {}", building.as_str())
            }
            Self::GreenhouseRequired(crop) => write!(f, "{crop} requires a greenhouse"),
            Self::FertilizerTooWeak {
                fertilizer,
                target_percent,
            } => write!(f, "{fertilizer} cannot reach {target_percent}% fertility"),
        }
    }
}

impl std::error::Error for FertilityError {}

fn rain_water_per_second() -> f64 {
    // The model uses 4 units of rain over the same 60-day month
    // scale used throughout this module, so convert that monthly offset into the
    // internal per-second rate before subtracting it from crop irrigation demand.
    4.0 / MONTH_SECONDS
}

fn crop_for_building<'a>(
    catalog: &'a CropCatalog,
    crop_name: &str,
    building: BuildingType,
) -> Result<&'a CropDefinition, FertilityError> {
    let crop = catalog
        .crops
        .get(crop_name)
        .ok_or_else(|| FertilityError::UnknownCrop(crop_name.to_owned()))?;
    if crop.requires_greenhouse && !matches!(building, BuildingType::FarmT3 | BuildingType::FarmT4) {
        return Err(FertilityError::GreenhouseRequired(crop_name.to_owned()));
    }
    if !crop.tiers.contains_key(&building) {
        return Err(FertilityError::UnsupportedCrop {
            crop: crop_name.to_owned(),
            building,
        });
    }
    Ok(crop)
}

pub fn build_rotation_summary(
    catalog: &CropCatalog,
    rotation: &[&str],
    building: BuildingType,
) -> Result<RotationSummary, FertilityError> {
    if rotation.is_empty() {
        return Ok(RotationSummary {
            total_duration_seconds: 0,
            natural_equilibrium_percent: 100.0,
            average_positive_drain_per_second: 0.0,
            water_per_month: 0.0,
            base_monthly_yields_at_100: BTreeMap::new(),
            effective_yield_per_month_at_100: 0.0,
        });
    }

    let resolved: Vec<&CropDefinition> = rotation
        .iter()
        .map(|crop_name| crop_for_building(catalog, crop_name, building))
        .collect::<Result<_, _>>()?;

    let mut total_seconds = 0_u32;
    let mut total_fertility_scaled_seconds = 0_i64;
    let mut positive_drain_scaled_seconds = 0_i64;
    let mut total_added_water_per_second = 0.0;
    let mut base_monthly_yields_at_100 = BTreeMap::new();

    for (index, crop) in resolved.iter().enumerate() {
        let prev = if index == 0 {
            resolved.last().copied()
        } else {
            resolved.get(index - 1).copied()
        };
        let mut fertility_value = crop.tiers[&building].fertility_per_second_scaled;
        if let Some(prev_crop) = prev {
            if prev_crop.name == crop.name && fertility_value > 0 {
                fertility_value = ((fertility_value as f64) * 1.5).round() as i32;
            }
        }
        total_seconds += crop.duration_seconds;
        total_fertility_scaled_seconds += fertility_value as i64 * crop.duration_seconds as i64;
        if fertility_value > 0 {
            positive_drain_scaled_seconds += fertility_value as i64 * crop.duration_seconds as i64;
        }
    }

    let average_fertility_scaled = total_fertility_scaled_seconds as f64 / total_seconds as f64;
    let natural_equilibrium_percent = 100.0 - average_fertility_scaled / 10.0;
    let average_positive_drain_per_second =
        positive_drain_scaled_seconds as f64 / total_seconds as f64 / 1000.0;

    for crop in &resolved {
        let tier = &crop.tiers[&building];
        let crop_monthly_yield_at_100 = tier.yield_per_cycle * MONTH_SECONDS / total_seconds as f64;
        *base_monthly_yields_at_100
            .entry(crop.name.clone())
            .or_insert(0.0) += crop_monthly_yield_at_100;

        let rotation_share = crop.duration_seconds as f64 / total_seconds as f64;
        total_added_water_per_second +=
            (tier.water_per_cycle / crop.duration_seconds as f64 - rain_water_per_second()).max(0.0)
                * rotation_share;
    }

    let effective_yield_per_month_at_100 = base_monthly_yields_at_100.values().sum::<f64>();

    Ok(RotationSummary {
        total_duration_seconds: total_seconds,
        natural_equilibrium_percent,
        average_positive_drain_per_second,
        water_per_month: total_added_water_per_second * MONTH_SECONDS,
        base_monthly_yields_at_100,
        effective_yield_per_month_at_100,
    })
}

pub fn build_rotation_summaries_batch(
    catalog: &CropCatalog,
    rotations: &[Vec<&str>],
    building: BuildingType,
) -> Result<BTreeMap<Vec<String>, RotationSummary>, FertilityError> {
    let mut summaries = BTreeMap::new();
    for rotation in rotations {
        let key = rotation.iter().map(|crop| (*crop).to_owned()).collect::<Vec<_>>();
        let summary = build_rotation_summary(catalog, rotation, building)?;
        summaries.insert(key, summary);
    }
    Ok(summaries)
}

pub fn calculate_fertilizer_requirement(
    actual_fertility: f64,
    natural_equilibrium: f64,
    positive_fertility_drain_per_second: f64,
    fertilizer: FertilizerProduct,
) -> Result<FertilizerRequirement, FertilityError> {
    if actual_fertility <= natural_equilibrium {
        return Ok(FertilizerRequirement {
            fertilizer_name: None,
            fertilizer_quantity_per_second: 0.0,
            fertilizer_required_per_month: 0.0,
        });
    }

    if fertilizer.max_fertility_percent < actual_fertility {
        return Err(FertilityError::FertilizerTooWeak {
            fertilizer: fertilizer.name,
            target_percent: actual_fertility,
        });
    }

    let natural_replenishment_per_second =
        (100.0 - actual_fertility) * NATURAL_REPLENISHMENT_PER_SECOND_PER_PERCENT;
    let fertilizer_quantity_per_second =
        (positive_fertility_drain_per_second - natural_replenishment_per_second)
        .max(0.0)
        / fertilizer.fertility_per_quantity_percent;

    Ok(FertilizerRequirement {
        fertilizer_name: Some(fertilizer.name),
        fertilizer_quantity_per_second,
        fertilizer_required_per_month: fertilizer_quantity_per_second * MONTH_SECONDS,
    })
}

pub fn simulate_rotation(
    catalog: &CropCatalog,
    rotation: &[&str],
    building: BuildingType,
    fertility_target: Option<f64>,
    fertilizer: Option<FertilizerProduct>,
) -> Result<SimulationResult, FertilityError> {
    if rotation.is_empty() {
        return Ok(SimulationResult {
            total_duration_seconds: 0,
            fertility_equilibrium: 100.0,
            average_actual_fertility: 100.0,
            fertilizer_required_per_month: 0.0,
            water_per_month: 0.0,
            yield_per_month_raw: 0.0,
            effective_yield_per_month: 0.0,
            individual_effective_yields: BTreeMap::new(),
        });
    }

    let summary = build_rotation_summary(catalog, rotation, building)?;
    let actual_fertility = fertility_target.unwrap_or(summary.natural_equilibrium_percent);
    let fertilizer_requirement = fertilizer
        .map(|product| {
            calculate_fertilizer_requirement(
                actual_fertility,
                summary.natural_equilibrium_percent,
                summary.average_positive_drain_per_second,
                product,
            )
        })
        .transpose()?
        .unwrap_or_else(|| FertilizerRequirement {
            fertilizer_name: None,
            fertilizer_quantity_per_second: 0.0,
            fertilizer_required_per_month: 0.0,
        });

    let fertility_factor = (actual_fertility / 100.0).max(0.0);
    let mut individual_effective_yields = BTreeMap::new();
    for crop_name in rotation {
        let crop_effective_yield = summary.base_monthly_yields_at_100[*crop_name] * fertility_factor;
        *individual_effective_yields
            .entry((*crop_name).to_owned())
            .or_insert(0.0) += crop_effective_yield;
    }

    Ok(SimulationResult {
        total_duration_seconds: summary.total_duration_seconds,
        fertility_equilibrium: summary.natural_equilibrium_percent,
        average_actual_fertility: actual_fertility,
        fertilizer_required_per_month: fertilizer_requirement.fertilizer_required_per_month,
        water_per_month: summary.water_per_month,
        yield_per_month_raw: summary.effective_yield_per_month_at_100,
        effective_yield_per_month: summary.effective_yield_per_month_at_100 * fertility_factor,
        individual_effective_yields,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::crop::BuildingType;
    use crate::io::wiki::load_crop_catalog;

    use super::{
        build_rotation_summaries_batch, build_rotation_summary, calculate_fertilizer_requirement,
        simulate_rotation, FertilityError, FERTILIZER_I, FERTILIZER_ORGANIC,
    };

    fn load_catalog() -> crate::domain::crop::CropCatalog {
        load_crop_catalog(&PathBuf::from("data/wiki_crop_data.json")).expect("catalog should load")
    }

    #[test]
    fn soybean_vegetables_summary_matches_expected_equilibrium() {
        let catalog = load_catalog();
        let summary = build_rotation_summary(
            &catalog,
            &["Soybean", "Vegetables"],
            BuildingType::FarmT4,
        )
        .expect("summary should build");

        assert!((summary.natural_equilibrium_percent - 73.45).abs() < 0.01);
        assert!((summary.average_positive_drain_per_second - 0.2655).abs() < 0.0001);
        assert!((summary.water_per_month - 38.5683625).abs() < 0.0001);
        assert!((summary.effective_yield_per_month_at_100 - 15.375).abs() < 0.0001);
    }

    #[test]
    fn potatoes_vegetables_rotation_matches_monthly_maintenance_drain() {
        let catalog = load_catalog();
        let summary = build_rotation_summary(
            &catalog,
            &["Potatoes", "Vegetables"],
            BuildingType::FarmT2,
        )
        .expect("summary should build");

        let fertilizer = calculate_fertilizer_requirement(
            100.0,
            summary.natural_equilibrium_percent,
            summary.average_positive_drain_per_second,
            FERTILIZER_ORGANIC,
        )
        .expect("fertilizer should calculate");

        assert!((summary.natural_equilibrium_percent - 82.5).abs() < 0.01);
        assert!((summary.average_positive_drain_per_second - 0.175).abs() < 0.0001);
        assert!((summary.water_per_month - 29.7793).abs() < 0.0001);
        assert!((fertilizer.fertilizer_required_per_month - 10.5).abs() < 0.001);
    }

    #[test]
    fn potatoes_vegetables_rotation_80_percent_reduces_fertilizer_need() {
        let catalog = load_catalog();
        let summary = build_rotation_summary(
            &catalog,
            &["Potatoes", "Vegetables"],
            BuildingType::FarmT2,
        )
        .expect("summary should build");

        let fertilizer = calculate_fertilizer_requirement(
            80.0,
            summary.natural_equilibrium_percent,
            summary.average_positive_drain_per_second,
            FERTILIZER_ORGANIC,
        )
        .expect("fertilizer should calculate");

        assert!((fertilizer.fertilizer_required_per_month - 0.0).abs() < 0.001);
    }

    #[test]
    fn fertilizer_requirement_increases_with_target_fertility() {
        let natural_equilibrium = 65.0;
        let positive_drain_per_second = 0.35;

        let fert_80 = calculate_fertilizer_requirement(
            80.0,
            natural_equilibrium,
            positive_drain_per_second,
            FERTILIZER_I,
        )
        .expect("80 should work");
        let fert_100 = calculate_fertilizer_requirement(
            100.0,
            natural_equilibrium,
            positive_drain_per_second,
            FERTILIZER_I,
        )
        .expect("100 should work");
        let fert_120 = calculate_fertilizer_requirement(
            120.0,
            natural_equilibrium,
            positive_drain_per_second,
            FERTILIZER_I,
        )
        .expect("120 should work");

        assert!((fert_80.fertilizer_required_per_month - 4.5).abs() < 0.0001);
        assert!((fert_100.fertilizer_required_per_month - 10.5).abs() < 0.0001);
        assert!((fert_120.fertilizer_required_per_month - 16.5).abs() < 0.0001);
        assert!(fert_80.fertilizer_required_per_month < fert_100.fertilizer_required_per_month);
        assert!(fert_100.fertilizer_required_per_month < fert_120.fertilizer_required_per_month);
    }

    #[test]
    fn non_greenhouse_crop_requirement_is_enforced() {
        let catalog = load_catalog();
        let error = build_rotation_summary(&catalog, &["Sugar Cane"], BuildingType::FarmT2)
            .expect_err("FarmT2 sugar cane should fail");

        match error {
            FertilityError::GreenhouseRequired(_) | FertilityError::UnsupportedCrop { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn fertilizer_cap_is_enforced_by_product() {
        let error = calculate_fertilizer_requirement(120.0, 47.5, 0.5, FERTILIZER_ORGANIC)
            .expect_err("organic should not support 120%");

        match error {
            FertilityError::FertilizerTooWeak { .. } => {}
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn simulate_rotation_matches_unfertilized_greenhouse_outputs() {
        let catalog = load_catalog();
        let simulation = simulate_rotation(
            &catalog,
            &["Soybean", "Vegetables"],
            BuildingType::FarmT4,
            None,
            None,
        )
        .expect("simulation should build");

        assert!((simulation.fertility_equilibrium - 73.45).abs() < 0.01);
        assert!((simulation.water_per_month - 38.5683625).abs() < 0.0001);
        assert!((simulation.yield_per_month_raw - 15.375).abs() < 0.0001);
        assert!((simulation.effective_yield_per_month - 11.2929).abs() < 0.01);
        assert!((simulation.individual_effective_yields["Soybean"] - 3.0298).abs() < 0.01);
        assert!((simulation.individual_effective_yields["Vegetables"] - 8.2631).abs() < 0.01);
        assert!((simulation.fertilizer_required_per_month - 0.0).abs() < 0.0001);
    }

    #[test]
    fn simulate_rotation_matches_fertilized_farm_outputs() {
        let catalog = load_catalog();
        let simulation = simulate_rotation(
            &catalog,
            &["Potatoes", "Vegetables"],
            BuildingType::FarmT2,
            Some(100.0),
            Some(FERTILIZER_ORGANIC),
        )
        .expect("simulation should build");

        assert!((simulation.fertility_equilibrium - 82.5).abs() < 0.01);
        assert!((simulation.water_per_month - 29.7793).abs() < 0.0001);
        assert!((simulation.yield_per_month_raw - 16.857142).abs() < 0.001);
        assert!((simulation.effective_yield_per_month - 16.857142).abs() < 0.001);
        assert!((simulation.individual_effective_yields["Potatoes"] - 8.285714).abs() < 0.001);
        assert!((simulation.individual_effective_yields["Vegetables"] - 8.571428).abs() < 0.001);
        assert!((simulation.fertilizer_required_per_month - 10.5).abs() < 0.001);
    }

    #[test]
    fn repeated_crop_rotation_has_higher_drain_than_single_crop_rotation() {
        let catalog = load_catalog();
        let alternating = build_rotation_summary(&catalog, &["Corn", "Wheat"], BuildingType::FarmT2)
            .expect("alternating summary should build");
        let repeated = build_rotation_summary(&catalog, &["Wheat", "Wheat"], BuildingType::FarmT2)
            .expect("repeated summary should build");

        assert!(repeated.average_positive_drain_per_second > alternating.average_positive_drain_per_second);
        assert!(repeated.natural_equilibrium_percent < alternating.natural_equilibrium_percent);
    }

    #[test]
    fn batch_summary_matches_single_summary_results() {
        let catalog = load_catalog();
        let rotations = vec![
            vec!["Soybean", "Vegetables"],
            vec!["Potatoes", "Vegetables"],
            vec!["Corn", "Wheat"],
        ];
        let batch = build_rotation_summaries_batch(&catalog, &rotations, BuildingType::FarmT2)
            .expect("batch should build");

        for rotation in rotations {
            let key = rotation.iter().map(|crop| (*crop).to_owned()).collect::<Vec<_>>();
            let single = build_rotation_summary(&catalog, &rotation, BuildingType::FarmT2)
                .expect("single should build");
            let from_batch = batch.get(&key).expect("rotation should be in batch");

            assert!((from_batch.natural_equilibrium_percent - single.natural_equilibrium_percent).abs() < 0.0001);
            assert!((from_batch.average_positive_drain_per_second - single.average_positive_drain_per_second).abs() < 0.0001);
            assert!((from_batch.water_per_month - single.water_per_month).abs() < 0.0001);
            assert!((from_batch.effective_yield_per_month_at_100 - single.effective_yield_per_month_at_100).abs() < 0.0001);
        }
    }
}
