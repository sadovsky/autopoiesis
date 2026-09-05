//! The event stream: the ground truth of a run.
//!
//! Everything the metrics say is derived from these records and nothing else — the
//! analyzer reconstructs each node's genome from `Birth` and `Acquire` rather than
//! reading the live world. That is what makes `hgt run --metrics` and
//! `hgt analyze events.jsonl` the same computation instead of two implementations that
//! happen to agree, and it is why the log carries no field the world computed for the
//! analyzer's convenience.

use crate::gene::{Acquisition, GeneId, NodeId};
use serde::{Deserialize, Serialize};

/// Why a node died.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// Ran out of energy: upkeep, failed stressors, or the cost of its own genome.
    Starved,
    /// Killed by a phage.
    Lysed,
}

/// Why a transfer attempt failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The recipient already held the gene.
    Redundant,
    /// The restriction system cut it: the donor is a different strain.
    Restricted,
    /// The recipient could not pay to integrate it.
    Broke,
    /// The recipient's policy declined.
    Declined,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Event {
    /// The stressor changed.
    Epoch { tick: u32, hazard: u8 },
    /// A node appeared: a founder (`parent: None`) or a child. `genes` is the genome it
    /// started with, after copy mutation and plasmid loss.
    Birth { tick: u32, node: NodeId, parent: Option<NodeId>, strain: u8, genes: Vec<GeneId> },
    /// A node acquired a gene *after* it was born — always laterally.
    Acquire { tick: u32, node: NodeId, gene: GeneId, via: Acquisition, from: Option<NodeId> },
    /// A transfer attempt and its outcome, including the ones that were refused: the
    /// denominator for the barrier metric.
    Transfer {
        tick: u32,
        from: NodeId,
        to: NodeId,
        gene: GeneId,
        via: Acquisition,
        distance: u32,
        refusal: Option<Refusal>,
    },
    Death { tick: u32, node: NodeId, cause: Cause },
    /// One tick's aggregate outcome.
    Tick { tick: u32, hazard: u8, alive: u32, survived: u32, failed: u32, energy_mean: f64 },
}

impl Event {
    pub fn tick(&self) -> u32 {
        match *self {
            Event::Epoch { tick, .. }
            | Event::Birth { tick, .. }
            | Event::Acquire { tick, .. }
            | Event::Transfer { tick, .. }
            | Event::Death { tick, .. }
            | Event::Tick { tick, .. } => tick,
        }
    }
}
