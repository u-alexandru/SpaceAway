//! Helm seated mode. Implemented in Task 4.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmState {
    Standing,
    Seated,
}

pub struct HelmController;
