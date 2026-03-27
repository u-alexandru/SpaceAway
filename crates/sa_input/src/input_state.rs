use crate::keyboard::KeyboardState;
use crate::mouse::MouseState;

pub struct InputState {
    pub keyboard: KeyboardState,
    pub mouse: MouseState,
}

impl InputState {
    pub fn new() -> Self {
        Self { keyboard: KeyboardState::new(), mouse: MouseState::new() }
    }

    pub fn end_frame(&mut self) { self.mouse.clear_delta(); self.keyboard.end_frame(); }
}

impl Default for InputState { fn default() -> Self { Self::new() } }
