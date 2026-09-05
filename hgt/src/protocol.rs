//! What nodes say to each other.
//!
//! One JSON object per line. A compact binary framing would be smaller, but the point of
//! this sandbox is that gene transfer *is* network traffic, and a protocol you can watch
//! with `nc` while it happens is worth more here than a few saved bytes. The same
//! envelopes go through the in-process transport and over TCP, so a message is never
//! privileged by which transport carried it.
//!
//! Conjugation is a three-message handshake — `Offer`, `Request`, `Transfer` — because a
//! donor should not be able to push code into a node that did not ask for it. The
//! recipient decides, and can answer `Reject`.

use crate::event::Refusal;
use crate::gene::{Acquisition, Gene, GeneId, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum Message {
    /// Gossip: who I am and who I know. Keeps peer lists alive as nodes die.
    Hello { strain: u8, peers: Vec<NodeId> },
    /// "These mobile genes are available."
    Offer { strain: u8, genes: Vec<GeneId> },
    /// "Send me that one."
    Request { gene: GeneId },
    /// The gene itself — the actual code, on the wire.
    Transfer { strain: u8, gene: Gene, via: Acquisition },
    /// "I could not take it, and why."
    Reject { gene: GeneId, reason: Refusal },
    /// A dying node's last broadcast: its genes, free for the taking.
    Eulogy { strain: u8, genes: Vec<Gene> },
    /// A phage: a gene travelling on its own, with hops left before it decays. It carries
    /// the strain of the host that last packaged it, so the barrier still applies.
    Phage { gene: Gene, origin: NodeId, strain: u8, hops: u32 },
}

/// A message with its addressing. `from` is asserted by the sender, exactly as it would
/// be on a real network — a node can lie about it, and nothing here depends on it being
/// true beyond bookkeeping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub from: NodeId,
    pub to: NodeId,
    #[serde(flatten)]
    pub msg: Message,
}

impl Envelope {
    pub fn new(from: NodeId, to: NodeId, msg: Message) -> Envelope {
        Envelope { from, to, msg }
    }

    /// One line of wire format.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("envelopes serialize")
    }

    pub fn decode(line: &str) -> Result<Envelope, serde_json::Error> {
        serde_json::from_str(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_envelope_is_one_readable_line_that_round_trips() {
        let gene = Gene::new(vec![0x0a, 0x1d], 3, 8, true);
        let env = Envelope::new(3, 7, Message::Transfer { strain: 1, gene, via: Acquisition::Conjugation });
        let line = env.encode();
        assert!(!line.contains('\n'), "wire frames are one line: {line}");
        assert!(line.contains("\"msg\":\"transfer\""), "{line}");
        assert_eq!(Envelope::decode(&line).unwrap(), env);
    }
}
