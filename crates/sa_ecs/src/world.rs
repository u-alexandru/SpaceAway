use hecs::{Entity, World};

pub struct GameWorld {
    world: World,
}

impl GameWorld {
    pub fn new() -> Self {
        Self {
            world: World::new(),
        }
    }

    pub fn spawn(&mut self, components: impl hecs::DynamicBundle) -> Entity {
        self.world.spawn(components)
    }

    pub fn despawn(&mut self, entity: Entity) {
        let _ = self.world.despawn(entity);
    }

    pub fn inner(&self) -> &World {
        &self.world
    }

    pub fn inner_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl Default for GameWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct Position(f32, f32, f32);
    #[allow(dead_code)]
    struct Velocity(f32, f32, f32);

    #[test]
    fn spawn_and_query() {
        let mut world = GameWorld::new();
        let entity = world.spawn((Position(1.0, 2.0, 3.0), Velocity(0.1, 0.0, 0.0)));
        let pos = world.inner().get::<&Position>(entity).unwrap();
        assert_eq!(pos.0, 1.0);
    }

    #[test]
    fn spawn_multiple_and_count() {
        let mut world = GameWorld::new();
        world.spawn((Position(0.0, 0.0, 0.0),));
        world.spawn((Position(1.0, 1.0, 1.0),));
        world.spawn((Position(2.0, 2.0, 2.0),));
        let count = world.inner().query::<&Position>().iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn despawn_entity() {
        let mut world = GameWorld::new();
        let entity = world.spawn((Position(1.0, 2.0, 3.0),));
        world.despawn(entity);
        assert!(world.inner().get::<&Position>(entity).is_err());
    }
}
