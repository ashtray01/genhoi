use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EconomyState {
    pub civilian_factories: u32,
    pub military_factories: u32,
    pub dockyards: u32,
    pub fuel_ratio: f32,
    pub equipment: Vec<EquipmentState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EquipmentState {
    pub kind: String,
    pub stockpile: i64,
    pub daily_balance: f32,
    pub fulfillment: f32,
}
