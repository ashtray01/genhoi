mod air;
mod country;
mod diplomacy;
mod economy;
mod front;
mod military;
mod navy;
mod supply;
mod war;

pub use air::AirRegionState;
pub use country::CountryState;
pub use diplomacy::DiplomacyState;
pub use economy::{EconomyState, EquipmentState};
pub use front::{FrontState, Terrain};
pub use military::ArmyState;
pub use navy::NavalRegionState;
pub use supply::SupplyState;
pub use war::WarState;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameState {
    pub game_hour: u64,
    pub country: CountryState,
    pub economy: EconomyState,
    pub wars: Vec<WarState>,
    pub fronts: Vec<FrontState>,
    pub armies: Vec<ArmyState>,
    pub air_regions: Vec<AirRegionState>,
    pub naval_regions: Vec<NavalRegionState>,
    pub diplomacy: DiplomacyState,
    pub strategic_summary: String,
}

impl GameState {
    #[must_use]
    pub fn at_war(&self) -> bool {
        self.wars.iter().any(|war| war.active)
    }
}
