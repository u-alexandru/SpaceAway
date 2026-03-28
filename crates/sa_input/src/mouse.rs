pub struct MouseState {
    delta_x: f32,
    delta_y: f32,
    left_pressed: bool,
    left_just_pressed: bool,
    left_just_released: bool,
    /// Cursor position in physical pixels (None if cursor is outside window).
    cursor_x: f32,
    cursor_y: f32,
    cursor_valid: bool,
}

impl MouseState {
    pub fn new() -> Self {
        Self {
            delta_x: 0.0,
            delta_y: 0.0,
            left_pressed: false,
            left_just_pressed: false,
            left_just_released: false,
            cursor_x: 0.0,
            cursor_y: 0.0,
            cursor_valid: false,
        }
    }

    pub fn set_cursor_position(&mut self, x: f32, y: f32) {
        self.cursor_x = x;
        self.cursor_y = y;
        self.cursor_valid = true;
    }

    pub fn position(&self) -> Option<(f32, f32)> {
        if self.cursor_valid { Some((self.cursor_x, self.cursor_y)) } else { None }
    }

    pub fn accumulate_delta(&mut self, dx: f32, dy: f32) {
        self.delta_x += dx;
        self.delta_y += dy;
    }

    pub fn delta(&self) -> (f32, f32) { (self.delta_x, self.delta_y) }

    pub fn clear_delta(&mut self) {
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.left_just_pressed = false;
        self.left_just_released = false;
    }

    /// Call when left mouse button state changes.
    pub fn set_left_pressed(&mut self, pressed: bool) {
        if pressed && !self.left_pressed {
            self.left_just_pressed = true;
        }
        if !pressed && self.left_pressed {
            self.left_just_released = true;
        }
        self.left_pressed = pressed;
    }

    pub fn left_pressed(&self) -> bool { self.left_pressed }
    pub fn left_just_pressed(&self) -> bool { self.left_just_pressed }
    pub fn left_just_released(&self) -> bool { self.left_just_released }
}

impl Default for MouseState { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_delta_is_zero() {
        let mouse = MouseState::new();
        assert_eq!(mouse.delta(), (0.0, 0.0));
    }

    #[test]
    fn accumulate_and_clear() {
        let mut mouse = MouseState::new();
        mouse.accumulate_delta(10.0, -5.0);
        mouse.accumulate_delta(3.0, 2.0);
        assert_eq!(mouse.delta(), (13.0, -3.0));
        mouse.clear_delta();
        assert_eq!(mouse.delta(), (0.0, 0.0));
    }

    #[test]
    fn left_button_just_pressed() {
        let mut mouse = MouseState::new();
        assert!(!mouse.left_pressed());
        assert!(!mouse.left_just_pressed());
        mouse.set_left_pressed(true);
        assert!(mouse.left_pressed());
        assert!(mouse.left_just_pressed());
        mouse.clear_delta();
        assert!(!mouse.left_just_pressed()); // cleared
        assert!(mouse.left_pressed()); // still held
    }

    #[test]
    fn left_button_just_released() {
        let mut mouse = MouseState::new();
        mouse.set_left_pressed(true);
        mouse.clear_delta();
        mouse.set_left_pressed(false);
        assert!(!mouse.left_pressed());
        assert!(mouse.left_just_released());
    }
}
