pub mod world;
pub mod bodies;
pub mod colliders;

pub use world::PhysicsWorld;
pub use bodies::{spawn_dynamic_body, spawn_static_body, spawn_kinematic_body, body_position};
pub use colliders::{attach_box_collider, attach_sphere_collider, attach_capsule_collider, add_ground};
pub use rapier3d::prelude::{
    RigidBody, RigidBodyBuilder, RigidBodyHandle,
    Collider, ColliderBuilder, ColliderHandle,
};
