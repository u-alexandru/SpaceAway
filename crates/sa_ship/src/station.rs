//! Station definitions: where interactables are placed in the ship.
//!
//! Each station is a named location (Helm, Nav, Engineering, etc.) with
//! a list of interactable positions. Phase 5a only defines the cockpit.

use glam::Vec3;

/// Named stations aboard the ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Station {
    Cockpit,
    Navigation,
    Sensors,
    Engineering,
    EngineRoom,
}

/// Configuration for an interactable placement.
#[derive(Clone, Debug)]
pub struct InteractablePlacement {
    /// What kind of interactable to place.
    pub kind: PlacementKind,
    /// Position in ship local space.
    pub position: Vec3,
    /// Human-readable label.
    pub label: String,
    /// Collider half-extents for raycast detection.
    pub collider_half_extents: Vec3,
}

/// What to place.
#[derive(Clone, Debug)]
pub enum PlacementKind {
    Lever,
    ToggleButton,
    MomentaryButton,
    Switch { num_positions: u8 },
    Screen { width: f32, height: f32 },
    HelmSeat,
}

/// Configuration for a station.
#[derive(Clone, Debug)]
pub struct StationConfig {
    pub station: Station,
    pub interactables: Vec<InteractablePlacement>,
}

/// Cockpit station layout.
///
/// The cockpit mesh spans z=0 to z=4 in ship local space.
/// Floor is at y=-1.0. The helm seat is centered near the front.
///
/// Positions are in ship local space (relative to ship body origin at (0,0,0)):
/// - Helm seat: center of cockpit, slightly forward
/// - Thrust lever: right side of helm
/// - Engine button: left side of helm
/// - Speed screen: above helm, slightly forward
pub fn cockpit_layout() -> StationConfig {
    StationConfig {
        station: Station::Cockpit,
        interactables: vec![
            InteractablePlacement {
                kind: PlacementKind::HelmSeat,
                position: Vec3::new(0.0, -0.5, 1.5),
                label: "Helm Seat".into(),
                collider_half_extents: Vec3::new(0.3, 0.4, 0.3),
            },
            InteractablePlacement {
                kind: PlacementKind::Lever,
                position: Vec3::new(0.6, -0.2, 1.2),
                label: "Thrust Lever".into(),
                collider_half_extents: Vec3::new(0.08, 0.2, 0.05),
            },
            InteractablePlacement {
                kind: PlacementKind::ToggleButton,
                position: Vec3::new(-0.6, -0.2, 1.2),
                label: "Engine On/Off".into(),
                collider_half_extents: Vec3::new(0.06, 0.06, 0.04),
            },
            InteractablePlacement {
                kind: PlacementKind::Screen {
                    width: 0.4,
                    height: 0.25,
                },
                position: Vec3::new(0.0, 0.3, 0.8),
                label: "Speed Display".into(),
                collider_half_extents: Vec3::new(0.2, 0.125, 0.02),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cockpit_has_four_interactables() {
        let layout = cockpit_layout();
        assert_eq!(layout.interactables.len(), 4);
    }

    #[test]
    fn cockpit_has_helm_seat() {
        let layout = cockpit_layout();
        let has_helm = layout
            .interactables
            .iter()
            .any(|i| matches!(i.kind, PlacementKind::HelmSeat));
        assert!(has_helm, "cockpit should have a helm seat");
    }

    #[test]
    fn cockpit_has_thrust_lever() {
        let layout = cockpit_layout();
        let has_lever = layout
            .interactables
            .iter()
            .any(|i| matches!(i.kind, PlacementKind::Lever) && i.label == "Thrust Lever");
        assert!(has_lever, "cockpit should have a thrust lever");
    }

    #[test]
    fn cockpit_has_engine_button() {
        let layout = cockpit_layout();
        let has_button = layout
            .interactables
            .iter()
            .any(|i| matches!(i.kind, PlacementKind::ToggleButton) && i.label == "Engine On/Off");
        assert!(has_button, "cockpit should have an engine on/off button");
    }

    #[test]
    fn cockpit_has_speed_screen() {
        let layout = cockpit_layout();
        let has_screen = layout
            .interactables
            .iter()
            .any(|i| matches!(i.kind, PlacementKind::Screen { .. }) && i.label == "Speed Display");
        assert!(has_screen, "cockpit should have a speed display");
    }

    #[test]
    fn all_positions_inside_cockpit() {
        let layout = cockpit_layout();
        for i in &layout.interactables {
            // Cockpit interior is roughly x=-2..2, y=-1..1.2, z=0..4
            assert!(
                i.position.x.abs() < 2.5
                    && i.position.y > -1.5
                    && i.position.y < 1.5
                    && i.position.z > 0.0
                    && i.position.z < 4.0,
                "interactable '{}' at {:?} should be inside cockpit bounds",
                i.label,
                i.position
            );
        }
    }
}
