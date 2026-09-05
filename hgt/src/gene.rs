//! Genes, and what a node knows about how it got them.
//!
//! A gene's identity is the FNV-1a hash of its code, not a name someone assigned. That
//! is the load-bearing choice in the whole sandbox: "the same gene" becomes a fact about
//! the bytes, so allele frequency, fixation and the transfer graph are all well defined
//! without a registry, and a single mutated byte is honestly a *different* allele rather
//! than a damaged copy of the old one.
//!
//! A genome is kept sorted by gene id so that iteration order — and therefore which gene
//! answers a stressor, and the world hash — does not depend on the order copies arrived
//! over the network.

use rand::RngExt;
use serde::{Deserialize, Serialize};

/// A node's identity. Nodes live in `node.rs`; the alias is here because a gene records
/// the node it was first synthesised in.
pub type NodeId = u32;

/// A gene's identity: FNV-1a over its code.
pub type GeneId = u64;

/// FNV-1a, as in the grid crate's `Grid::hash` — one hash function for the whole repo.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A gene: a short program, plus where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gene {
    pub id: GeneId,
    #[serde(with = "hex_code")]
    pub code: Vec<u8>,
    /// The node the gene was first synthesised or mutated into existence in.
    pub origin: NodeId,
    /// The tick that happened on.
    pub birth_tick: u32,
    /// Mobile genes sit on a plasmid and can be donated; immobile ones are chromosomal.
    pub mobile: bool,
}

impl Gene {
    pub fn new(code: Vec<u8>, origin: NodeId, birth_tick: u32, mobile: bool) -> Gene {
        Gene { id: fnv1a(&code), code, origin, birth_tick, mobile }
    }

    /// Short human-facing name: the first 6 hex digits of the id.
    pub fn short(&self) -> String {
        format!("{:06x}", self.id & 0xff_ffff)
    }
}

/// Short name for a gene id, for records that carry the id but not the gene.
pub fn short_id(id: GeneId) -> String {
    format!("{:06x}", id & 0xff_ffff)
}

/// How a node came to hold a gene. Everything except `Birth` and `Founder` is lateral —
/// the distinction the whole sandbox exists to measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Acquisition {
    /// Present in the initial population.
    Founder,
    /// Inherited from a parent at fission.
    Birth,
    /// Pushed by a donor over a direct connection.
    Conjugation,
    /// Taken up from a dead node's released fragments.
    Transformation,
    /// Delivered by a phage.
    Transduction,
}

impl Acquisition {
    /// Did this gene arrive sideways rather than down the tree?
    pub fn is_lateral(self) -> bool {
        matches!(self, Acquisition::Conjugation | Acquisition::Transformation | Acquisition::Transduction)
    }

    pub fn glyph(self) -> char {
        match self {
            Acquisition::Founder => 'o',
            Acquisition::Birth => 'v',
            Acquisition::Conjugation => 'c',
            Acquisition::Transformation => 't',
            Acquisition::Transduction => 'p',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Acquisition::Founder => "founder",
            Acquisition::Birth => "birth",
            Acquisition::Conjugation => "conjugation",
            Acquisition::Transformation => "transformation",
            Acquisition::Transduction => "transduction",
        }
    }
}

/// A gene as held by one node, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    pub gene: Gene,
    pub via: Acquisition,
    /// The donor, for laterally acquired genes; the parent, at birth.
    pub from: Option<NodeId>,
    /// The tick this node acquired it.
    pub since: u32,
}

/// One node's genes, sorted by id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genome {
    genes: Vec<Carried>,
}

impl Genome {
    pub fn new() -> Genome {
        Genome { genes: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.genes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.genes.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Carried> {
        self.genes.iter()
    }

    /// Total code size — what upkeep is charged on.
    pub fn bytes(&self) -> usize {
        self.genes.iter().map(|c| c.gene.code.len()).sum()
    }

    pub fn contains(&self, id: GeneId) -> bool {
        self.genes.binary_search_by_key(&id, |c| c.gene.id).is_ok()
    }

    pub fn get(&self, id: GeneId) -> Option<&Carried> {
        self.genes.binary_search_by_key(&id, |c| c.gene.id).ok().map(|i| &self.genes[i])
    }

    pub fn ids(&self) -> impl Iterator<Item = GeneId> + '_ {
        self.genes.iter().map(|c| c.gene.id)
    }

    /// The genes this node could donate.
    pub fn mobile(&self) -> impl Iterator<Item = &Carried> + '_ {
        self.genes.iter().filter(|c| c.gene.mobile)
    }

    /// Insert unless already held. Returns false if the gene was already there — a
    /// transfer that lands on a node that already has the gene is not an acquisition.
    pub fn insert(&mut self, carried: Carried) -> bool {
        match self.genes.binary_search_by_key(&carried.gene.id, |c| c.gene.id) {
            Ok(_) => false,
            Err(i) => {
                self.genes.insert(i, carried);
                true
            }
        }
    }

    pub fn remove(&mut self, id: GeneId) -> Option<Carried> {
        match self.genes.binary_search_by_key(&id, |c| c.gene.id) {
            Ok(i) => Some(self.genes.remove(i)),
            Err(_) => None,
        }
    }
}

/// Copy `code` with each byte replaced at probability `rate`. Returns `None` if nothing
/// changed, so callers can keep the parent's gene identity rather than re-hashing.
pub fn mutate<R: RngExt>(code: &[u8], rate: f64, rng: &mut R) -> Option<Vec<u8>> {
    if rate <= 0.0 {
        return None;
    }
    let mut out = code.to_vec();
    let mut changed = false;
    for b in &mut out {
        if rng.random::<f64>() < rate {
            *b = rng.random::<u8>();
            changed = true;
        }
    }
    changed.then_some(out)
}

/// `Vec<u8>` as a hex string, so an event log or a wire frame stays readable.
mod hex_code {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(code: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(code.len() * 2);
        for b in code {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 {
            return Err(serde::de::Error::custom("odd-length gene code"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    #[test]
    fn identity_is_the_code_and_a_genome_stays_sorted() {
        let a = Gene::new(vec![1, 2, 3], 0, 0, true);
        let b = Gene::new(vec![1, 2, 3], 9, 40, false);
        assert_eq!(a.id, b.id, "identical code must be the same gene whoever holds it");
        assert_ne!(a.id, Gene::new(vec![1, 2, 4], 0, 0, true).id);

        let mut g = Genome::new();
        for code in [vec![9], vec![1], vec![5]] {
            let gene = Gene::new(code, 0, 0, true);
            assert!(g.insert(Carried { gene, via: Acquisition::Founder, from: None, since: 0 }));
        }
        let ids: Vec<GeneId> = g.ids().collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "genome must not depend on arrival order");

        let dup = Gene::new(vec![5], 3, 7, true);
        assert!(!g.insert(Carried { gene: dup, via: Acquisition::Conjugation, from: Some(3), since: 7 }));
        assert_eq!(g.len(), 3);
    }

    #[test]
    fn mutation_is_off_at_rate_zero_and_changes_the_gene_id_otherwise() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
        let code = vec![0u8; 64];
        assert_eq!(mutate(&code, 0.0, &mut rng), None);
        let m = mutate(&code, 0.5, &mut rng).expect("64 bytes at p=0.5 must change something");
        assert_ne!(fnv1a(&m), fnv1a(&code));
        assert_eq!(m.len(), code.len(), "mutation is substitution, not indel");
    }

    #[test]
    fn gene_code_round_trips_through_json_as_hex() {
        let g = Gene::new(vec![0x0a, 0xff, 0x00, 0x7d], 2, 5, true);
        let s = serde_json::to_string(&g).unwrap();
        assert!(s.contains("\"0aff007d\""), "gene code should be readable hex: {s}");
        assert_eq!(serde_json::from_str::<Gene>(&s).unwrap(), g);
    }
}
