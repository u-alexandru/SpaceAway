pub mod deposits;
pub mod resources;
pub mod suit;

pub use deposits::{ResourceDeposit, ResourceKind, generate_deposits};
pub use resources::ShipResources;
pub use suit::SuitResources;
