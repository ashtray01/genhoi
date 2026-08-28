use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WarState {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub war_score: f32,
    pub own_casualties: u64,
    pub enemy_casualties: u64,
}
