pub mod world;
pub mod bodies;
pub mod colliders;
pub mod forces;

pub use world::PhysicsWorld;
pub use bodies::{spawn_dynamic_body, spawn_static_body, spawn_kinematic_body, body_position};
pub use colliders::{attach_box_collider, attach_sphere_collider, attach_capsule_collider, add_ground};
pub use forces::{apply_force, apply_impulse, apply_torque, linear_velocity};
pub use rapier3d::prelude::{
    RigidBody, RigidBodyBuilder, RigidBodyHandle,
    Collider, ColliderBuilder, ColliderHandle,
    QueryFilter, Ray,
};
