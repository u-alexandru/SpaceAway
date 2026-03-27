use crate::coords::{LocalPos, WorldPos};

pub fn world_to_local(world: WorldPos, origin: WorldPos) -> LocalPos {
    LocalPos::new(
        (world.x - origin.x) as f32,
        (world.y - origin.y) as f32,
        (world.z - origin.z) as f32,
    )
}

pub fn local_to_world(local: LocalPos, origin: WorldPos) -> WorldPos {
    WorldPos::new(
        origin.x + local.x as f64,
        origin.y + local.y as f64,
        origin.z + local.z as f64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldPos;

    #[test]
    fn rebase_at_origin() {
        let local = world_to_local(WorldPos::new(10.0, 20.0, 30.0), WorldPos::ORIGIN);
        assert!((local.x - 10.0).abs() < 1e-5);
    }

    #[test]
    fn rebase_cancels_origin() {
        let local = world_to_local(
            WorldPos::new(1000.0, 2000.0, 3000.0),
            WorldPos::new(1000.0, 2000.0, 3000.0),
        );
        assert!(local.x.abs() < 1e-5);
    }

    #[test]
    fn rebase_large_coordinates() {
        let local = world_to_local(
            WorldPos::new(1.5e11, 0.0, 100.0),
            WorldPos::new(1.5e11, 0.0, 0.0),
        );
        assert!(local.x.abs() < 1e-3);
        assert!((local.z - 100.0).abs() < 1e-3);
    }
}
