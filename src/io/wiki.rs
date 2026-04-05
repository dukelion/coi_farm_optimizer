use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::domain::crop::{BuildingType, CropCatalog, CropDefinition, TierMetrics};

#[derive(Debug)]
pub enum WikiLoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl Display for WikiLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for WikiLoadError {}

impl From<std::io::Error> for WikiLoadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for WikiLoadError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Deserialize)]
struct WikiPayload {
    crops_used_by_simulator: Vec<WikiCropEntry>,
}

#[derive(Debug, Deserialize)]
struct WikiCropEntry {
    name: String,
    #[serde(alias = "duration_days")]
    duration_seconds: Option<u32>,
    created_in: Option<Vec<String>>,
    base_output_per_cycle: Option<f64>,
    base_output_per_cycle_tier_i: Option<f64>,
    base_output_per_cycle_tier_ii: Option<f64>,
    tiers: BTreeMap<String, WikiTierEntry>,
    loader_overrides: Option<WikiLoaderOverrides>,
}

#[derive(Debug, Deserialize, Default)]
struct WikiTierEntry {
    water_per_cycle: Option<f64>,
    water_per_month: Option<f64>,
    organic_fertilizer_per_cycle: Option<f64>,
    organic_fertilizer_per_month: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct WikiLoaderOverrides {
    base_output_per_cycle: Option<f64>,
    requires_greenhouse: Option<bool>,
    yield_per_farm: Option<BTreeMap<String, f64>>,
    water_per_farm: Option<BTreeMap<String, f64>>,
    fertility_per_farm: Option<BTreeMap<String, i32>>,
}

fn parse_building_type(value: &str) -> Option<BuildingType> {
    match value {
        "FarmT1" => Some(BuildingType::FarmT1),
        "FarmT2" => Some(BuildingType::FarmT2),
        "FarmT3" => Some(BuildingType::FarmT3),
        "FarmT4" => Some(BuildingType::FarmT4),
        _ => None,
    }
}

fn base_output_for_entry(entry: &WikiCropEntry) -> Option<f64> {
    entry
        .base_output_per_cycle
        .or(entry.loader_overrides.as_ref().and_then(|o| o.base_output_per_cycle))
        .or_else(|| {
            let mut inferred = Vec::new();
            if let Some(value) = entry.base_output_per_cycle_tier_i {
                inferred.push(value / 1.25);
            }
            if let Some(value) = entry.base_output_per_cycle_tier_ii {
                inferred.push(value / 1.50);
            }
            (!inferred.is_empty()).then(|| inferred.iter().sum::<f64>() / inferred.len() as f64)
        })
}

fn requires_greenhouse(entry: &WikiCropEntry) -> bool {
    if let Some(value) = entry
        .loader_overrides
        .as_ref()
        .and_then(|o| o.requires_greenhouse)
    {
        return value;
    }

    let created_in = entry.created_in.clone().unwrap_or_default();
    let created_in: BTreeSet<_> = created_in.into_iter().collect();
    !created_in.is_empty() && created_in.iter().all(|name| matches!(name.as_str(), "Greenhouse" | "Greenhouse II"))
}

pub fn load_crop_catalog(path: &Path) -> Result<CropCatalog, WikiLoadError> {
    let text = fs::read_to_string(path)?;
    load_crop_catalog_from_str(&text)
}

pub fn load_embedded_crop_catalog() -> Result<CropCatalog, WikiLoadError> {
    load_crop_catalog_from_str(include_str!("../../data/wiki_crop_data.json"))
}

fn load_crop_catalog_from_str(text: &str) -> Result<CropCatalog, WikiLoadError> {
    let payload: WikiPayload = serde_json::from_str(&text)?;
    let mut crops = BTreeMap::new();

    for entry in payload.crops_used_by_simulator {
        let Some(duration_seconds) = entry.duration_seconds else {
            continue;
        };
        let Some(base_output_per_cycle) = base_output_for_entry(&entry) else {
            continue;
        };

        let mut tiers = BTreeMap::new();

        if let Some(overrides) = &entry.loader_overrides {
            if let (Some(yields), Some(water), Some(fertility)) = (
                overrides.yield_per_farm.as_ref(),
                overrides.water_per_farm.as_ref(),
                overrides.fertility_per_farm.as_ref(),
            ) {
                for building in BuildingType::ALL {
                    let key = building.as_str();
                    let Some(yield_per_cycle) = yields.get(key) else {
                        continue;
                    };
                    let Some(water_per_second) = water.get(key) else {
                        continue;
                    };
                    let Some(fertility_per_second_scaled) = fertility.get(key) else {
                        continue;
                    };
                    tiers.insert(
                        building,
                        TierMetrics {
                            yield_per_cycle: *yield_per_cycle,
                            water_per_cycle: *water_per_second * duration_seconds as f64,
                            organic_fertilizer_per_cycle: (*fertility_per_second_scaled as f64 / 1000.0)
                                * duration_seconds as f64,
                            water_per_month: *water_per_second * 60.0,
                            organic_fertilizer_per_month: (*fertility_per_second_scaled as f64 / 1000.0)
                                * 60.0,
                            fertility_per_second_scaled: *fertility_per_second_scaled,
                        },
                    );
                }
            }
        }

        for (building_name, tier) in &entry.tiers {
            let Some(building) = parse_building_type(building_name) else {
                continue;
            };
            let (
                Some(water_per_cycle),
                Some(water_per_month),
                Some(organic_fertilizer_per_cycle),
                Some(organic_fertilizer_per_month),
            ) = (
                tier.water_per_cycle,
                tier.water_per_month,
                tier.organic_fertilizer_per_cycle,
                tier.organic_fertilizer_per_month,
            )
            else {
                continue;
            };

            let yield_per_cycle = match building {
                BuildingType::FarmT1 | BuildingType::FarmT2 => base_output_per_cycle,
                BuildingType::FarmT3 => entry.base_output_per_cycle_tier_i.unwrap_or(base_output_per_cycle * 1.25),
                BuildingType::FarmT4 => entry.base_output_per_cycle_tier_ii.unwrap_or(base_output_per_cycle * 1.50),
            };

            tiers.insert(
                building,
                TierMetrics {
                    yield_per_cycle,
                    water_per_cycle,
                    organic_fertilizer_per_cycle,
                    water_per_month,
                    organic_fertilizer_per_month,
                    fertility_per_second_scaled: ((organic_fertilizer_per_cycle / duration_seconds as f64) * 1000.0)
                        .round() as i32,
                },
            );
        }

        if tiers.is_empty() {
            continue;
        }

        let crop_name = entry.name.clone();
        let crop_requires_greenhouse = requires_greenhouse(&entry);
        crops.insert(
            crop_name.clone(),
            CropDefinition {
                name: crop_name,
                duration_seconds,
                requires_greenhouse: crop_requires_greenhouse,
                tiers,
            },
        );
    }

    Ok(CropCatalog { crops })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::load_crop_catalog;
    use crate::domain::crop::BuildingType;

    #[test]
    fn loads_fruit_and_green_manure_from_json() {
        let path = PathBuf::from("data/wiki_crop_data.json");
        let catalog = load_crop_catalog(&path).expect("catalog should load");

        assert!(catalog.crops.contains_key("Fruit"));
        assert!(catalog.crops.contains_key("Green Manure"));
        assert!(catalog.supports("Fruit", BuildingType::FarmT3));
        assert!(catalog.supports("Fruit", BuildingType::FarmT4));
        assert!(catalog.supports("Green Manure", BuildingType::FarmT2));
    }
}
