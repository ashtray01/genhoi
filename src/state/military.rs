use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArmyState {
    pub id: String,
    pub name: String,
    pub divisions: u32,
    pub reserve: bool,
    pub average_strength: f32,
    pub average_organization: f32,
}
