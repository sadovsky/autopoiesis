//! Autopoiesis: a simulation testing a minimal definition of computational life — a
//! region of a noisy, energy-limited substrate that actively maintains its own
//! encoding and boundary against decay. Life is measured, not declared.

pub mod config;
pub mod energy;
pub mod grid;
pub mod isa;
pub mod noise;
pub mod render;
pub mod sim;
pub mod snapshot;
pub mod vm;
