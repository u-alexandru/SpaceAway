use std::fmt;
use std::marker::PhantomData;

#[derive(Debug)]
pub struct Handle<T> {
    id: u64,
    _marker: PhantomData<T>,
}

impl<T> Handle<T> {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Display for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle({})", self.id)
    }
}

pub struct HandleGenerator {
    next_id: u64,
}

impl HandleGenerator {
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next<T>(&mut self) -> Handle<T> {
        let id = self.next_id;
        self.next_id += 1;
        Handle {
            id,
            _marker: PhantomData,
        }
    }
}

impl Default for HandleGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_unique() {
        let mut generator = HandleGenerator::new();
        let a: Handle<()> = generator.next();
        let b: Handle<()> = generator.next();
        assert_ne!(a, b);
    }

    #[test]
    fn handles_are_typed() {
        struct MeshMarker;
        struct TextureMarker;
        let mut generator = HandleGenerator::new();
        let _mesh: Handle<MeshMarker> = generator.next();
        let _tex: Handle<TextureMarker> = generator.next();
    }

    #[test]
    fn handle_display() {
        let mut generator = HandleGenerator::new();
        let h: Handle<()> = generator.next();
        assert_eq!(format!("{h}"), "Handle(0)");
    }
}
