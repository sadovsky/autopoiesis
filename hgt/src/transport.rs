//! How messages get from one node to another.
//!
//! One trait, two implementations. `SimTransport` runs in this process with modelled
//! latency, loss and partitions, and is deterministic given a seed — it is what the
//! tests, the sweep and every reported number use. `TcpTransport` (`tcp.rs`) puts the
//! same envelopes on real sockets between operating-system processes, which is not
//! deterministic and is not supposed to be: it is the demonstration that a gene really
//! does cross a network.
//!
//! `SimTransport` carries its own RNG, seeded away from the world's stream, so that
//! turning loss on does not shift every later draw in the simulation. Same trick as the
//! grid crate's analyzer: the measurement apparatus must not perturb the thing measured.

use crate::gene::NodeId;
use crate::protocol::Envelope;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::collections::BTreeMap;

pub trait Transport {
    /// Hand a message to the network. Delivery is never immediate.
    fn send(&mut self, env: Envelope);
    /// Everything that has arrived by `tick`, in the order it was sent.
    fn deliver(&mut self, tick: u32) -> Vec<Envelope>;
    /// Messages sent and messages dropped, for the run summary.
    fn counts(&self) -> (u64, u64);
    /// A node has gone; the transport may forget about it.
    fn forget(&mut self, _node: NodeId) {}
    /// The world announces the tick at the start of each network phase, so `send` needs
    /// no tick argument. A real socket does not care what tick it is.
    fn set_tick(&mut self, _tick: u32) {}
}

/// In-process delivery with a fixed delay, a drop rate, and cuttable links.
pub struct SimTransport {
    latency: u32,
    loss: f64,
    /// Ordered links that are down. Cuts are symmetric.
    cuts: Vec<(NodeId, NodeId)>,
    queue: BTreeMap<u32, Vec<Envelope>>,
    now: u32,
    rng: Xoshiro256PlusPlus,
    sent: u64,
    dropped: u64,
}

impl SimTransport {
    pub fn new(latency: u32, loss: f64, seed: u64) -> SimTransport {
        SimTransport {
            latency: latency.max(1),
            loss,
            cuts: Vec::new(),
            queue: BTreeMap::new(),
            now: 0,
            // Decoupled from the world's stream so that loss does not reshuffle the run.
            rng: Xoshiro256PlusPlus::seed_from_u64(seed ^ 0x6e65_7477_6f72_6b00),
            sent: 0,
            dropped: 0,
        }
    }

    /// The tick a message sent now will arrive: at least one tick away, because a node
    /// reacts to what has arrived, never to what it is in the middle of sending.
    pub fn arrival(&self, tick: u32) -> u32 {
        tick + self.latency
    }

    /// Latency in ticks, at least 1.
    pub fn latency(&self) -> u32 {
        self.latency
    }

    /// Cut the link between two nodes, in both directions.
    pub fn cut(&mut self, a: NodeId, b: NodeId) {
        let pair = (a.min(b), a.max(b));
        if !self.cuts.contains(&pair) {
            self.cuts.push(pair);
        }
    }

    /// Cut every link between `group` and everything else: a network partition.
    pub fn partition(&mut self, group: &[NodeId], all: &[NodeId]) {
        for &a in group {
            for &b in all {
                if !group.contains(&b) {
                    self.cut(a, b);
                }
            }
        }
    }

    pub fn heal(&mut self) {
        self.cuts.clear();
    }

    fn is_cut(&self, a: NodeId, b: NodeId) -> bool {
        let pair = (a.min(b), a.max(b));
        self.cuts.contains(&pair)
    }

    pub fn in_flight(&self) -> usize {
        self.queue.values().map(|v| v.len()).sum()
    }
}

impl Transport for SimTransport {
    fn send(&mut self, env: Envelope) {
        self.sent += 1;
        if self.is_cut(env.from, env.to) {
            self.dropped += 1;
            return;
        }
        if self.loss > 0.0 && self.rng.random::<f64>() < self.loss {
            self.dropped += 1;
            return;
        }
        // The queue is keyed by arrival tick; `now` was set by the world at the start of
        // this tick's network phase.
        let at = self.now + self.latency;
        self.queue.entry(at).or_default().push(env);
    }

    fn deliver(&mut self, tick: u32) -> Vec<Envelope> {
        let due: Vec<u32> = self.queue.range(..=tick).map(|(k, _)| *k).collect();
        let mut out = Vec::new();
        for k in due {
            if let Some(v) = self.queue.remove(&k) {
                out.extend(v);
            }
        }
        out
    }

    fn counts(&self) -> (u64, u64) {
        (self.sent, self.dropped)
    }

    fn forget(&mut self, node: NodeId) {
        for v in self.queue.values_mut() {
            v.retain(|e| e.to != node);
        }
    }

    fn set_tick(&mut self, tick: u32) {
        self.now = tick;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Message;

    fn hello(from: NodeId, to: NodeId) -> Envelope {
        Envelope::new(from, to, Message::Hello { strain: 0, peers: vec![] })
    }

    #[test]
    fn latency_delays_delivery_and_order_is_preserved() {
        let mut t = SimTransport::new(2, 0.0, 1);
        Transport::set_tick(&mut t, 10);
        t.send(hello(1, 2));
        t.send(hello(3, 4));
        assert!(t.deliver(10).is_empty());
        assert!(t.deliver(11).is_empty(), "a two-tick message must not arrive in one");
        let got = t.deliver(12);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].from, got[1].from), (1, 3), "delivery order follows send order");
        assert_eq!(t.in_flight(), 0);
    }

    #[test]
    fn loss_and_cuts_drop_messages_deterministically() {
        let run = || {
            let mut t = SimTransport::new(1, 0.5, 42);
            Transport::set_tick(&mut t, 0);
            for i in 0..200 {
                t.send(hello(i, i + 1000));
            }
            t.counts()
        };
        let (sent, dropped) = run();
        assert_eq!(sent, 200);
        assert!(dropped > 60 && dropped < 140, "dropped {dropped}/200 at loss 0.5");
        assert_eq!(run(), (sent, dropped), "the same seed must drop the same messages");

        let mut t = SimTransport::new(1, 0.0, 1);
        Transport::set_tick(&mut t, 0);
        t.cut(1, 2);
        t.send(hello(1, 2));
        t.send(hello(2, 1));
        t.send(hello(1, 3));
        assert_eq!(t.deliver(5).len(), 1, "a cut link is down in both directions");
        assert_eq!(t.counts(), (3, 2));
    }
}
