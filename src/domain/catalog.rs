use std::collections::{BTreeMap, BTreeSet};

use crate::domain::crop::{BuildingType, CropCatalog};
use crate::domain::fertility::{
    simulate_rotation, FertilityError, FertilizerProduct, SimulationResult,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rotation(pub Vec<String>);

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogOption {
    pub rotation: Rotation,
    pub fertility_target: Option<f64>,
    pub simulation: SimulationResult,
}

pub fn has_adjacent_repeat(rotation: &[String]) -> bool {
    if rotation.is_empty() {
        return false;
    }
    (0..rotation.len()).any(|idx| rotation[idx] == rotation[(idx + 1) % rotation.len()])
}

pub fn reduce_periodic_rotation(rotation: &[String]) -> Vec<String> {
    if rotation.is_empty() {
        return Vec::new();
    }
    for period in 1..rotation.len() {
        if !rotation.len().is_multiple_of(period) {
            continue;
        }
        let candidate = &rotation[..period];
        let matches = rotation
            .chunks(period)
            .all(|chunk| chunk == candidate);
        if matches {
            return candidate.to_vec();
        }
    }
    rotation.to_vec()
}

pub fn canonical_rotation(rotation: &[String]) -> Rotation {
    let reduced = reduce_periodic_rotation(rotation);
    let mut best = reduced.clone();
    for offset in 1..reduced.len() {
        let rotated = reduced[offset..]
            .iter()
            .chain(reduced[..offset].iter())
            .cloned()
            .collect::<Vec<_>>();
        if rotated < best {
            best = rotated;
        }
    }
    Rotation(best)
}

fn enumerate_rotations(
    crops: &[String],
    length: usize,
    current: &mut Vec<String>,
    output: &mut BTreeSet<Rotation>,
) {
    if current.len() == length {
        if !has_adjacent_repeat(current) {
            output.insert(canonical_rotation(current));
        }
        return;
    }

    for crop in crops {
        current.push(crop.clone());
        enumerate_rotations(crops, length, current, output);
        current.pop();
    }
}

pub fn generate_base_rotations(crops: &[String]) -> Vec<Rotation> {
    let mut sorted = crops.to_vec();
    sorted.sort();
    let mut rotations = BTreeSet::new();
    for crop in &sorted {
        rotations.insert(Rotation(vec![crop.clone()]));
    }
    for length in 2..=4 {
        enumerate_rotations(&sorted, length, &mut Vec::new(), &mut rotations);
    }
    rotations.into_iter().collect()
}

pub fn rotations_for_building(
    catalog: &CropCatalog,
    crops: &[String],
    building: BuildingType,
) -> Vec<Rotation> {
    generate_base_rotations(crops)
        .into_iter()
        .filter(|rotation| {
            rotation.0.iter().all(|crop_name| {
                catalog.supports(crop_name, building)
                    && catalog
                        .crops
                        .get(crop_name)
                        .map(|crop| !crop.requires_greenhouse || matches!(building, BuildingType::FarmT3 | BuildingType::FarmT4))
                        .unwrap_or(false)
            })
        })
        .collect()
}

pub fn build_option_catalog(
    catalog: &CropCatalog,
    crops: &[String],
    building: BuildingType,
    fertility_levels: &[Option<f64>],
    fertilizer: Option<FertilizerProduct>,
) -> Result<Vec<CatalogOption>, FertilityError> {
    let rotations = rotations_for_building(catalog, crops, building);
    let mut options = Vec::new();
    for rotation in rotations {
        for fertility_target in fertility_levels {
            let rotation_refs = rotation.0.iter().map(String::as_str).collect::<Vec<_>>();
            let simulation = simulate_rotation(
                catalog,
                &rotation_refs,
                building,
                *fertility_target,
                fertilizer,
            )?;
            options.push(CatalogOption {
                rotation: rotation.clone(),
                fertility_target: *fertility_target,
                simulation,
            });
        }
    }
    Ok(options)
}

pub fn build_catalog_by_building(
    catalog: &CropCatalog,
    crops: &[String],
    fertility_levels_by_building: &BTreeMap<BuildingType, Vec<Option<f64>>>,
    fertilizer: Option<FertilizerProduct>,
) -> Result<BTreeMap<BuildingType, Vec<CatalogOption>>, FertilityError> {
    let mut result = BTreeMap::new();
    for building in BuildingType::ALL {
        let fertility_levels = fertility_levels_by_building
            .get(&building)
            .cloned()
            .unwrap_or_else(|| vec![None]);
        result.insert(
            building,
            build_option_catalog(catalog, crops, building, &fertility_levels, fertilizer)?,
        );
    }
    Ok(result)
}

pub fn build_baseline_catalog_by_building(
    catalog: &CropCatalog,
    crops: &[String],
    baseline_fertility_by_building: &BTreeMap<BuildingType, Option<f64>>,
    fertilizer: Option<FertilizerProduct>,
) -> Result<BTreeMap<BuildingType, Vec<CatalogOption>>, FertilityError> {
    let fertility_levels_by_building = baseline_fertility_by_building
        .iter()
        .map(|(building, baseline)| (*building, vec![*baseline]))
        .collect::<BTreeMap<_, _>>();
    build_catalog_by_building(catalog, crops, &fertility_levels_by_building, fertilizer)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::domain::crop::BuildingType;
    use crate::domain::fertility::FERTILIZER_ORGANIC;
    use crate::io::wiki::load_crop_catalog;

    use super::{
        build_baseline_catalog_by_building, build_catalog_by_building, build_option_catalog,
        canonical_rotation, generate_base_rotations, reduce_periodic_rotation, rotations_for_building,
        Rotation,
    };

    fn load_catalog() -> crate::domain::crop::CropCatalog {
        load_crop_catalog(&PathBuf::from("data/wiki_crop_data.json")).expect("catalog should load")
    }

    #[test]
    fn reduce_periodic_rotation_collapses_repeated_sequences() {
        let rotation = vec![
            "Potatoes".to_owned(),
            "Vegetables".to_owned(),
            "Potatoes".to_owned(),
            "Vegetables".to_owned(),
        ];
        let reduced = reduce_periodic_rotation(&rotation);
        assert_eq!(reduced, vec!["Potatoes".to_owned(), "Vegetables".to_owned()]);
    }

    #[test]
    fn canonical_rotation_chooses_lexicographically_smallest_offset() {
        let rotation = vec![
            "Vegetables".to_owned(),
            "Potatoes".to_owned(),
            "Corn".to_owned(),
        ];
        assert_eq!(
            canonical_rotation(&rotation),
            Rotation(vec![
                "Corn".to_owned(),
                "Vegetables".to_owned(),
                "Potatoes".to_owned(),
            ])
        );
    }

    #[test]
    fn generate_base_rotations_removes_adjacent_repeats_and_periodic_duplicates() {
        let rotations = generate_base_rotations(&[
            "Potatoes".to_owned(),
            "Vegetables".to_owned(),
        ]);

        assert!(rotations.contains(&Rotation(vec!["Potatoes".to_owned()])));
        assert!(rotations.contains(&Rotation(vec![
            "Potatoes".to_owned(),
            "Vegetables".to_owned()
        ])));
        assert!(!rotations.contains(&Rotation(vec![
            "Potatoes".to_owned(),
            "Potatoes".to_owned()
        ])));
        assert!(!rotations.contains(&Rotation(vec![
            "Potatoes".to_owned(),
            "Vegetables".to_owned(),
            "Potatoes".to_owned(),
            "Vegetables".to_owned()
        ])));
    }

    #[test]
    fn greenhouse_filtering_excludes_fruit_from_farm_t2() {
        let catalog = load_catalog();
        let crops = vec!["Fruit".to_owned(), "Corn".to_owned()];

        let farm_rotations = rotations_for_building(&catalog, &crops, BuildingType::FarmT2);
        let greenhouse_rotations = rotations_for_building(&catalog, &crops, BuildingType::FarmT3);

        assert!(farm_rotations.iter().all(|rotation| !rotation.0.contains(&"Fruit".to_owned())));
        assert!(greenhouse_rotations
            .iter()
            .any(|rotation| rotation.0.contains(&"Fruit".to_owned())));
    }

    #[test]
    fn option_catalog_expands_rotations_by_fertility_level() {
        let catalog = load_catalog();
        let options = build_option_catalog(
            &catalog,
            &["Potatoes".to_owned(), "Vegetables".to_owned()],
            BuildingType::FarmT2,
            &[Some(80.0), Some(100.0)],
            Some(FERTILIZER_ORGANIC),
        )
        .expect("catalog should build");

        assert!(!options.is_empty());
        assert!(options.iter().any(|option| option.fertility_target == Some(80.0)));
        assert!(options.iter().any(|option| option.fertility_target == Some(100.0)));
    }

    #[test]
    fn catalog_by_building_respects_building_support() {
        let catalog = load_catalog();
        let mut fertility_levels = BTreeMap::new();
        fertility_levels.insert(BuildingType::FarmT1, vec![None]);
        fertility_levels.insert(BuildingType::FarmT2, vec![Some(100.0)]);
        fertility_levels.insert(BuildingType::FarmT3, vec![Some(100.0)]);
        fertility_levels.insert(BuildingType::FarmT4, vec![Some(100.0)]);

        let catalog_by_building = build_catalog_by_building(
            &catalog,
            &["Fruit".to_owned(), "Corn".to_owned()],
            &fertility_levels,
            Some(FERTILIZER_ORGANIC),
        )
        .expect("catalog should build");

        assert!(catalog_by_building[&BuildingType::FarmT2]
            .iter()
            .all(|option| !option.rotation.0.contains(&"Fruit".to_owned())));
        assert!(catalog_by_building[&BuildingType::FarmT3]
            .iter()
            .any(|option| option.rotation.0.contains(&"Fruit".to_owned())));
    }

    #[test]
    fn baseline_catalog_by_building_keeps_only_one_fertility_target_per_building() {
        let catalog = load_catalog();
        let baseline = BTreeMap::from([
            (BuildingType::FarmT2, Some(80.0)),
            (BuildingType::FarmT3, Some(100.0)),
            (BuildingType::FarmT4, None),
        ]);

        let catalog_by_building = build_baseline_catalog_by_building(
            &catalog,
            &["Potatoes".to_owned(), "Vegetables".to_owned(), "Fruit".to_owned()],
            &baseline,
            Some(FERTILIZER_ORGANIC),
        )
        .expect("baseline catalog should build");

        assert!(catalog_by_building[&BuildingType::FarmT2]
            .iter()
            .all(|option| option.fertility_target == Some(80.0)));
        assert!(catalog_by_building[&BuildingType::FarmT3]
            .iter()
            .all(|option| option.fertility_target == Some(100.0)));
        assert!(catalog_by_building[&BuildingType::FarmT4]
            .iter()
            .all(|option| option.fertility_target.is_none()));
    }
}
