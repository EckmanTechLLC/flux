// State engine and entity management (Task 3)

mod engine;
mod entity;

pub use engine::StateEngine;
pub use entity::{Entity, StateUpdate};

#[cfg(test)]
mod tests;
