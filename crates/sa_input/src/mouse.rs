pub struct MouseState {
    delta_x: f32,
    delta_y: f32,
}

impl MouseState {
    pub fn new() -> Self { Self { delta_x: 0.0, delta_y: 0.0 } }

    pub fn accumulate_delta(&mut self, dx: f32, dy: f32) {
        self.delta_x += dx;
        self.delta_y += dy;
    }

    pub fn delta(&self) -> (f32, f32) { (self.delta_x, self.delta_y) }

    pub fn clear_delta(&mut self) { self.delta_x = 0.0; self.delta_y = 0.0; }
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
}
