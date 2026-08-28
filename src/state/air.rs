use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AirRegionState {
    pub id: String,
    pub name: String,
    pub friendly_aircraft: u32,
    pub enemy_estimated_aircraft: u32,
    pub superiority: f32,
}
