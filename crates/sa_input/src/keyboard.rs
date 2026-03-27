use std::collections::HashSet;
use winit::keyboard::KeyCode;

pub struct KeyboardState {
    pressed: HashSet<KeyCode>,
}

impl KeyboardState {
    pub fn new() -> Self { Self { pressed: HashSet::new() } }

    pub fn set_pressed(&mut self, key: KeyCode, pressed: bool) {
        if pressed { self.pressed.insert(key); } else { self.pressed.remove(&key); }
    }

    pub fn is_pressed(&self, key: KeyCode) -> bool { self.pressed.contains(&key) }
}

impl Default for KeyboardState { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn key_not_pressed_by_default() {
        let kb = KeyboardState::new();
        assert!(!kb.is_pressed(KeyCode::KeyW));
    }

    #[test]
    fn press_and_release() {
        let mut kb = KeyboardState::new();
        kb.set_pressed(KeyCode::KeyW, true);
        assert!(kb.is_pressed(KeyCode::KeyW));
        kb.set_pressed(KeyCode::KeyW, false);
        assert!(!kb.is_pressed(KeyCode::KeyW));
    }

    #[test]
    fn multiple_keys() {
        let mut kb = KeyboardState::new();
        kb.set_pressed(KeyCode::KeyW, true);
        kb.set_pressed(KeyCode::KeyA, true);
        assert!(kb.is_pressed(KeyCode::KeyW));
        assert!(kb.is_pressed(KeyCode::KeyA));
        assert!(!kb.is_pressed(KeyCode::KeyS));
    }
}
