use std::time::Duration;

const MAX_DELTA: Duration = Duration::from_millis(100);

pub struct FrameTime {
    delta: Duration,
    total: Duration,
    frame_count: u64,
}

impl FrameTime {
    pub fn new() -> Self {
        Self {
            delta: Duration::ZERO,
            total: Duration::ZERO,
            frame_count: 0,
        }
    }

    pub fn advance(&mut self, elapsed: Duration) {
        self.delta = elapsed.min(MAX_DELTA);
        self.total += self.delta;
        self.frame_count += 1;
    }

    pub fn delta(&self) -> Duration {
        self.delta
    }

    pub fn delta_seconds(&self) -> f64 {
        self.delta.as_secs_f64()
    }

    pub fn total_seconds(&self) -> f64 {
        self.total.as_secs_f64()
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Default for FrameTime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn initial_state() {
        let time = FrameTime::new();
        assert_eq!(time.delta_seconds(), 0.0);
        assert_eq!(time.frame_count(), 0);
    }

    #[test]
    fn advance_updates_delta() {
        let mut time = FrameTime::new();
        time.advance(Duration::from_millis(16));
        assert!((time.delta_seconds() - 0.016).abs() < 1e-5);
        assert_eq!(time.frame_count(), 1);
    }

    #[test]
    fn advance_accumulates_total() {
        let mut time = FrameTime::new();
        time.advance(Duration::from_millis(16));
        time.advance(Duration::from_millis(16));
        assert!((time.total_seconds() - 0.032).abs() < 1e-5);
        assert_eq!(time.frame_count(), 2);
    }

    #[test]
    fn delta_clamped_to_max() {
        let mut time = FrameTime::new();
        time.advance(Duration::from_secs(1));
        assert!(time.delta_seconds() <= 0.1);
    }
}
