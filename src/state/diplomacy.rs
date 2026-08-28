use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DiplomacyState {
    pub faction: Option<String>,
    pub allies: Vec<String>,
    pub enemies: Vec<String>,
    pub world_tension: f32,
}
