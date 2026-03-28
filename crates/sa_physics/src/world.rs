use rapier3d::prelude::*;

/// Wraps the rapier3d physics pipeline into a single convenient struct.
pub struct PhysicsWorld {
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    gravity: nalgebra::Vector3<f32>,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,
}

impl PhysicsWorld {
    /// Creates a new physics world with Earth-like gravity (0, -9.81, 0).
    pub fn new() -> Self {
        Self::with_gravity(0.0, -9.81, 0.0)
    }

    /// Creates a new physics world with custom gravity.
    /// Use `with_gravity(0.0, 0.0, 0.0)` for zero-g space environments.
    pub fn with_gravity(x: f32, y: f32, z: f32) -> Self {
        Self {
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            gravity: nalgebra::Vector3::new(x, y, z),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
        }
    }

    /// Advances the physics simulation by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
    }

    /// Inserts a rigid body and returns its handle.
    pub fn add_rigid_body(&mut self, body: RigidBody) -> RigidBodyHandle {
        self.rigid_body_set.insert(body)
    }

    /// Attaches a collider to a rigid body and returns the collider handle.
    pub fn add_collider(
        &mut self,
        collider: Collider,
        parent: RigidBodyHandle,
    ) -> ColliderHandle {
        self.collider_set
            .insert_with_parent(collider, parent, &mut self.rigid_body_set)
    }

    /// Inserts a collider without a parent rigid body.
    pub fn add_collider_without_parent(&mut self, collider: Collider) -> ColliderHandle {
        self.collider_set.insert(collider)
    }

    /// Returns a reference to a rigid body, if the handle is valid.
    pub fn get_body(&self, handle: RigidBodyHandle) -> Option<&RigidBody> {
        self.rigid_body_set.get(handle)
    }

    /// Returns a mutable reference to a rigid body, if the handle is valid.
    pub fn get_body_mut(&mut self, handle: RigidBodyHandle) -> Option<&mut RigidBody> {
        self.rigid_body_set.get_mut(handle)
    }

    /// Returns the current gravity vector.
    pub fn gravity(&self) -> (f32, f32, f32) {
        (self.gravity.x, self.gravity.y, self.gravity.z)
    }

    /// Sets the gravity vector.
    pub fn set_gravity(&mut self, x: f32, y: f32, z: f32) {
        self.gravity = nalgebra::Vector3::new(x, y, z);
    }

    /// Updates the query pipeline for raycasting. Call after `step()`.
    pub fn update_query_pipeline(&mut self) {
        self.query_pipeline.update(&self.collider_set);
    }

    /// Cast a ray and return the first hit collider handle and distance.
    /// `filter` controls which colliders are considered (e.g., sensors only).
    /// Returns None if no hit within `max_toi`.
    pub fn cast_ray(
        &self,
        origin: nalgebra::Point3<f32>,
        direction: nalgebra::Vector3<f32>,
        max_toi: f32,
        solid: bool,
        filter: QueryFilter,
    ) -> Option<(ColliderHandle, f32)> {
        let ray = Ray::new(origin, direction);
        self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &ray,
            max_toi,
            solid,
            filter,
        )
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_world_has_gravity() {
        let world = PhysicsWorld::new();
        let (_, y, _) = world.gravity();
        assert!(y < 0.0, "default gravity y should be negative");
    }

    #[test]
    fn step_does_not_panic() {
        let mut world = PhysicsWorld::new();
        world.step(1.0 / 60.0);
    }

    #[test]
    fn zero_gravity_option() {
        let world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let (_, y, _) = world.gravity();
        assert!((y - 0.0).abs() < f32::EPSILON, "zero-g world should have y=0");
    }
}
