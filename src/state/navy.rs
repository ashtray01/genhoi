use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NavalRegionState {
    pub id: String,
    pub name: String,
    pub friendly_supremacy: f32,
    pub convoy_efficiency: f32,
}
