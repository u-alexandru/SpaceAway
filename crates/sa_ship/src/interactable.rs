use rapier3d::prelude::ColliderHandle;

/// The kind of interactable object and its state.
#[derive(Clone, Debug)]
pub enum InteractableKind {
    /// Analog control, click-and-drag. position: 0.0 (bottom) to 1.0 (top).
    Lever { position: f32 },

    /// Discrete click. Momentary = active while held, Toggle = click on/off.
    Button { pressed: bool, mode: ButtonMode },

    /// Multi-position, click to cycle. Wraps around.
    Switch { position: u8, num_positions: u8 },

    /// Display only. No interaction.
    Screen { text_lines: Vec<String> },

    /// Helm seat. Click to enter seated mode.
    HelmSeat,
}

/// Button behavior mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonMode {
    /// Active only while mouse is held down.
    Momentary,
    /// Toggles on/off with each click.
    Toggle,
}

/// An interactable object in the ship.
#[derive(Clone, Debug)]
pub struct Interactable {
    pub kind: InteractableKind,
    /// The sensor collider used for raycast detection.
    pub collider_handle: ColliderHandle,
    /// Human-readable label for UI hints.
    pub label: String,
}

impl Interactable {
    /// Create a new lever interactable at position 0.0.
    pub fn lever(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Lever { position: 0.0 },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new toggle button interactable.
    pub fn toggle_button(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Button {
                pressed: false,
                mode: ButtonMode::Toggle,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new momentary button interactable.
    pub fn momentary_button(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Button {
                pressed: false,
                mode: ButtonMode::Momentary,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new switch interactable.
    pub fn switch(
        collider_handle: ColliderHandle,
        num_positions: u8,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind: InteractableKind::Switch {
                position: 0,
                num_positions,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new screen interactable.
    pub fn screen(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Screen {
                text_lines: Vec::new(),
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a helm seat interactable.
    pub fn helm_seat(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::HelmSeat,
            collider_handle,
            label: label.into(),
        }
    }

    // --- State manipulation ---

    /// Set lever position, clamped to 0.0..=1.0. No-op if not a lever.
    pub fn set_lever_position(&mut self, pos: f32) {
        if let InteractableKind::Lever { position } = &mut self.kind {
            *position = pos.clamp(0.0, 1.0);
        }
    }

    /// Get lever position. Returns None if not a lever.
    pub fn lever_position(&self) -> Option<f32> {
        if let InteractableKind::Lever { position } = &self.kind {
            Some(*position)
        } else {
            None
        }
    }

    /// Press a button. For Toggle: flips state. For Momentary: sets pressed = true.
    pub fn press_button(&mut self) {
        if let InteractableKind::Button { pressed, mode } = &mut self.kind {
            match mode {
                ButtonMode::Toggle => *pressed = !*pressed,
                ButtonMode::Momentary => *pressed = true,
            }
        }
    }

    /// Release a button. For Momentary: sets pressed = false. Toggle: no-op.
    pub fn release_button(&mut self) {
        if let InteractableKind::Button { pressed, mode } = &mut self.kind
            && *mode == ButtonMode::Momentary
        {
            *pressed = false;
        }
    }

    /// Get button pressed state. Returns None if not a button.
    pub fn is_button_pressed(&self) -> Option<bool> {
        if let InteractableKind::Button { pressed, .. } = &self.kind {
            Some(*pressed)
        } else {
            None
        }
    }

    /// Advance switch to next position (wraps around). No-op if not a switch.
    pub fn cycle_switch(&mut self) {
        if let InteractableKind::Switch {
            position,
            num_positions,
        } = &mut self.kind
        {
            *position = (*position + 1) % *num_positions;
        }
    }

    /// Get switch position. Returns None if not a switch.
    pub fn switch_position(&self) -> Option<u8> {
        if let InteractableKind::Switch { position, .. } = &self.kind {
            Some(*position)
        } else {
            None
        }
    }

    /// Update screen text lines. No-op if not a screen.
    pub fn set_screen_text(&mut self, lines: Vec<String>) {
        if let InteractableKind::Screen { text_lines } = &mut self.kind {
            *text_lines = lines;
        }
    }

    /// Returns true if this is a helm seat.
    pub fn is_helm_seat(&self) -> bool {
        matches!(self.kind, InteractableKind::HelmSeat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::ColliderHandle;

    /// Helper: create a dummy collider handle for testing.
    fn dummy_handle() -> ColliderHandle {
        // ColliderHandle is an Index; use from_raw_parts with generation 0
        ColliderHandle::from_raw_parts(0, 0)
    }

    #[test]
    fn lever_position_clamped_low() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(-0.5);
        assert_eq!(lever.lever_position(), Some(0.0));
    }

    #[test]
    fn lever_position_clamped_high() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(1.5);
        assert_eq!(lever.lever_position(), Some(1.0));
    }

    #[test]
    fn lever_position_normal() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(0.75);
        assert!((lever.lever_position().unwrap() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn lever_initial_position_zero() {
        let lever = Interactable::lever(dummy_handle(), "throttle");
        assert_eq!(lever.lever_position(), Some(0.0));
    }

    #[test]
    fn toggle_button_flips() {
        let mut btn = Interactable::toggle_button(dummy_handle(), "engine");
        assert_eq!(btn.is_button_pressed(), Some(false));
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(true));
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(false));
    }

    #[test]
    fn toggle_button_release_is_noop() {
        let mut btn = Interactable::toggle_button(dummy_handle(), "engine");
        btn.press_button(); // true
        btn.release_button(); // should stay true (toggle ignores release)
        assert_eq!(btn.is_button_pressed(), Some(true));
    }

    #[test]
    fn momentary_button_held() {
        let mut btn = Interactable::momentary_button(dummy_handle(), "fire");
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(true));
        btn.release_button();
        assert_eq!(btn.is_button_pressed(), Some(false));
    }

    #[test]
    fn switch_cycles_through_positions() {
        let mut sw = Interactable::switch(dummy_handle(), 3, "mode");
        assert_eq!(sw.switch_position(), Some(0));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(1));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(2));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(0)); // wraps
    }

    #[test]
    fn screen_text_update() {
        let mut screen = Interactable::screen(dummy_handle(), "speed");
        screen.set_screen_text(vec!["Speed: 0 m/s".into()]);
        if let InteractableKind::Screen { text_lines } = &screen.kind {
            assert_eq!(text_lines.len(), 1);
            assert_eq!(text_lines[0], "Speed: 0 m/s");
        } else {
            panic!("expected screen");
        }
    }

    #[test]
    fn helm_seat_detected() {
        let seat = Interactable::helm_seat(dummy_handle(), "helm");
        assert!(seat.is_helm_seat());
    }

    #[test]
    fn lever_position_on_non_lever_returns_none() {
        let btn = Interactable::toggle_button(dummy_handle(), "x");
        assert_eq!(btn.lever_position(), None);
    }

    #[test]
    fn button_pressed_on_non_button_returns_none() {
        let lever = Interactable::lever(dummy_handle(), "x");
        assert_eq!(lever.is_button_pressed(), None);
    }
}
