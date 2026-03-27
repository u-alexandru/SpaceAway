use std::any::{Any, TypeId};
use std::collections::HashMap;

pub struct EventBus {
    channels: HashMap<TypeId, Box<dyn Any>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    pub fn emit<T: 'static>(&mut self, event: T) {
        let type_id = TypeId::of::<T>();
        let channel = self
            .channels
            .entry(type_id)
            .or_insert_with(|| Box::new(Vec::<T>::new()));
        channel.downcast_mut::<Vec<T>>().unwrap().push(event);
    }

    pub fn read<T: 'static>(&self) -> impl Iterator<Item = &T> {
        self.channels
            .get(&TypeId::of::<T>())
            .and_then(|channel| channel.downcast_ref::<Vec<T>>())
            .map(|vec| vec.iter())
            .unwrap_or_else(|| [].iter())
    }

    pub fn flush(&mut self) {
        self.channels.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct DamageEvent {
        amount: f32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct HealEvent {
        amount: f32,
    }

    #[test]
    fn emit_and_read_events() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.emit(DamageEvent { amount: 5.0 });
        let events: Vec<&DamageEvent> = bus.read::<DamageEvent>().collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].amount, 10.0);
    }

    #[test]
    fn different_event_types_independent() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.emit(HealEvent { amount: 20.0 });
        assert_eq!(bus.read::<DamageEvent>().count(), 1);
        assert_eq!(bus.read::<HealEvent>().count(), 1);
    }

    #[test]
    fn flush_clears_events() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.flush();
        assert_eq!(bus.read::<DamageEvent>().count(), 0);
    }

    #[test]
    fn read_empty_returns_none() {
        let bus = EventBus::new();
        assert_eq!(bus.read::<DamageEvent>().count(), 0);
    }
}
