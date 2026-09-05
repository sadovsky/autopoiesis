//! A sandbox for horizontal gene transfer between programs: a population of software
//! nodes that face a shifting stressor, hold executable genes, and pass those genes to
//! each other over a network rather than only down the reproduction tree.
//!
//! Life is not the question here — spread is. The measurement that matters is how often
//! a trait shows up in a node whose parent never had it.

pub mod config;
pub mod event;
pub mod gene;
pub mod hazard;
pub mod isa;
pub mod metrics;
pub mod node;
pub mod protocol;
pub mod transport;
pub mod vm;
pub mod world;
