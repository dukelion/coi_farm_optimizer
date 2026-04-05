use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildingType {
    FarmT1,
    FarmT2,
    FarmT3,
    FarmT4,
}

impl BuildingType {
    pub const ALL: [Self; 4] = [Self::FarmT1, Self::FarmT2, Self::FarmT3, Self::FarmT4];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FarmT1 => "FarmT1",
            Self::FarmT2 => "FarmT2",
            Self::FarmT3 => "FarmT3",
            Self::FarmT4 => "FarmT4",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseBuildingTypeError(pub String);

impl Display for ParseBuildingTypeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown building type: {}", self.0)
    }
}

impl std::error::Error for ParseBuildingTypeError {}

impl FromStr for BuildingType {
    type Err = ParseBuildingTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "FarmT1" => Ok(Self::FarmT1),
            "FarmT2" => Ok(Self::FarmT2),
            "FarmT3" => Ok(Self::FarmT3),
            "FarmT4" => Ok(Self::FarmT4),
            other => Err(ParseBuildingTypeError(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TierMetrics {
    pub yield_per_cycle: f64,
    pub water_per_cycle: f64,
    pub organic_fertilizer_per_cycle: f64,
    pub water_per_month: f64,
    pub organic_fertilizer_per_month: f64,
    pub fertility_per_second_scaled: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CropDefinition {
    pub name: String,
    pub duration_seconds: u32,
    pub requires_greenhouse: bool,
    pub tiers: BTreeMap<BuildingType, TierMetrics>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CropCatalog {
    pub crops: BTreeMap<String, CropDefinition>,
}

impl CropCatalog {
    pub fn supports(&self, crop_name: &str, building: BuildingType) -> bool {
        self.crops
            .get(crop_name)
            .and_then(|crop| crop.tiers.get(&building))
            .is_some()
    }
}
