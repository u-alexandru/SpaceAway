use crate::world::GameWorld;
use sa_core::{EventBus, FrameTime};

type SystemFn = Box<dyn FnMut(&mut GameWorld, &mut EventBus, &FrameTime)>;

struct System {
    name: String,
    run: SystemFn,
}

pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    pub fn add_system(
        &mut self,
        name: &str,
        system: impl FnMut(&mut GameWorld, &mut EventBus, &FrameTime) + 'static,
    ) {
        self.systems.push(System {
            name: name.to_string(),
            run: Box::new(system),
        });
    }

    pub fn run(&mut self, world: &mut GameWorld, events: &mut EventBus, time: &FrameTime) {
        for system in &mut self.systems {
            log::trace!("Running system: {}", system.name);
            (system.run)(world, events, time);
        }
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn systems_run_in_order() {
        let order = Arc::new(AtomicU32::new(0));
        let mut schedule = Schedule::new();

        let o1 = Arc::clone(&order);
        schedule.add_system("first", move |_world, _events, _time| {
            assert_eq!(o1.fetch_add(1, Ordering::SeqCst), 0);
        });

        let o2 = Arc::clone(&order);
        schedule.add_system("second", move |_world, _events, _time| {
            assert_eq!(o2.fetch_add(1, Ordering::SeqCst), 1);
        });

        let mut world = crate::world::GameWorld::new();
        let mut events = sa_core::EventBus::new();
        let time = sa_core::FrameTime::new();
        schedule.run(&mut world, &mut events, &time);
        assert_eq!(order.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn empty_schedule_runs() {
        let mut schedule = Schedule::new();
        let mut world = crate::world::GameWorld::new();
        let mut events = sa_core::EventBus::new();
        let time = sa_core::FrameTime::new();
        schedule.run(&mut world, &mut events, &time);
    }
}
