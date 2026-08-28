use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Plains,
    Forest,
    Hills,
    Mountains,
    Urban,
    Marsh,
    Desert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontState {
    pub id: String,
    pub name: String,
    pub friendly_divisions: u32,
    pub enemy_estimated_divisions: u32,
    pub friendly_strength: f32,
    pub enemy_strength: f32,
    pub organization: f32,
    pub supply: f32,
    pub terrain: Terrain,
    pub front_width_km: f32,
    pub depth_km: f32,
    pub enemy_pressure: f32,
    pub enemy_pressure_north: f32,
    pub enemy_pressure_south: f32,
    pub friendly_pressure: f32,
    pub salient_depth_km: f32,
    pub salient_neck_width_km: f32,
    pub nearby_reserve_divisions: u32,
    pub recent_friendly_casualties: u64,
    pub recent_enemy_casualties: u64,
    pub recent_territory_delta_km2: i64,
    pub air_superiority: f32,
    pub equipment_ratio: f32,
    pub offensive_active: bool,
}
