use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CountryState {
    pub tag: String,
    pub name: String,
    pub manpower: u64,
    pub political_power: f32,
    pub stability: f32,
    pub war_support: f32,
}
