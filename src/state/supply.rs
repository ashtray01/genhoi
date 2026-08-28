use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SupplyState {
    pub delivered_ratio: f32,
    pub hub_access: f32,
    pub railway_bottleneck: bool,
}
