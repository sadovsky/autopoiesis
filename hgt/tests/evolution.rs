//! Genes can be found, not only moved — and the way a node behaves is itself heritable.
//!
//! These are the two things that make this a gym rather than a demonstration: a lineage
//! can climb to a working gene it never received, and a population can be invaded by a
//! different way of treating its neighbours.

use hgt::config::{FounderGenes, HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::gene::{Acquisition, Carried, Gene};
use hgt::hazard::{Environment, compile_resistance};
use hgt::metrics::{Analyzer, Record};
use hgt::world::World;

/// A world with one stressor that never shifts, a survivable baseline income, and no
/// seeded answers: the regime a search happens in.
fn search_world() -> HgtConfig {
    HgtConfig {
        nodes: 48,
        max_nodes: 96,
        hazard_kinds: 1,
        epoch_ticks: 1_000_000,
        founder_genes: FounderGenes::Random,
        income: 10,
        damage: 4,
        reward: 40,
        probes: 4,
        hazard_gradient: 1.0,
        mutation_rate: 0.02,
        mechanisms: Mechanisms::none(),
        ..HgtConfig::default()
    }
}

/// Put the whole population `bits` bit-flips away from a gene that works.
fn seed_near_miss(world: &mut World, env: &Environment, bits: u32) {
    let (key, rot) = env.secret(0);
    let mut mask = 0u32;
    for i in 0..bits {
        mask |= 1 << ((i * 7 + 3) % 32);
    }
    let code = compile_resistance(key ^ mask, rot);
    for node in world.nodes.values_mut() {
        node.genome = Default::default();
        let gene = Gene::new(code.clone(), node.id, 0, true);
        node.genome.insert(Carried::new(gene, Acquisition::Founder, None, 0, false));
    }
}

#[test]
fn a_population_climbs_to_a_gene_no_one_gave_it() {
    let cfg = search_world();
    let seed = 3;
    let env = Environment::new(&cfg, seed);
    let mut world = World::new(cfg, seed).expect("valid config");
    seed_near_miss(&mut world, &env, 4);

    let mut discovery = None;
    for _ in 0..3_000 {
        for e in world.step() {
            if let Event::Discovery { tick, gene, kind, .. } = e {
                discovery.get_or_insert((tick, gene, kind));
            }
        }
        if discovery.is_some() || world.extinct() {
            break;
        }
    }
    let (tick, gene, kind) = discovery.expect("nobody ever found the gene");
    assert_eq!(kind, 0);
    assert!(tick > 0, "the founders were given a near miss, not the answer");
    assert!(world.solvers(0) > 0, "the gene that was found should be in the population");

    // What was found computes the answer; that is the whole test of "found".
    let code = world
        .nodes
        .values()
        .flat_map(|n| n.genome.iter())
        .find(|c| c.gene.id == gene)
        .map(|c| c.gene.code.clone())
        .expect("the discovered gene is held by someone");
    assert!(env.solves(&code, 0, 96));

    // And with no gradient there is nothing to climb: the same population, scored
    // all-or-nothing, never gets anywhere.
    let flat = HgtConfig { hazard_gradient: 0.0, ..search_world() };
    let env = Environment::new(&flat, seed);
    let mut world = World::new(flat, seed).expect("valid config");
    seed_near_miss(&mut world, &env, 4);
    let mut found = false;
    for _ in 0..3_000 {
        found |= world.step().iter().any(|e| matches!(e, Event::Discovery { .. }));
        if found || world.extinct() {
            break;
        }
    }
    assert!(!found, "a flat landscape should not be climbable");
}

#[test]
fn a_way_of_behaving_is_inherited_and_can_be_counted() {
    let cfg = HgtConfig {
        nodes: 32,
        max_nodes: 96,
        epoch_ticks: 150,
        hazard_kinds: 3,
        selfish_founders: 8,
        hazard_gradient: 0.0,
        mechanisms: Mechanisms::default(),
        ..HgtConfig::default()
    };
    let seed = 6;
    let mut analyzer = Analyzer::new(&cfg, seed);
    let mut world = World::new(cfg, seed).expect("valid config");
    let mut frames = Vec::new();
    for e in world.founding_events() {
        analyzer.observe(&e);
    }
    for _ in 0..400 {
        for e in world.step() {
            for rec in analyzer.observe(&e) {
                if let Record::Frame(f) = rec {
                    frames.push(*f);
                }
            }
        }
        if world.extinct() {
            break;
        }
    }

    let first = frames.first().expect("at least one frame");
    assert_eq!(first.policies.selfish, 8, "eight founders were dropped in as free riders");
    assert!(first.policies.always_accept > 0);

    // Policy is a trait of a node, not a setting of the run: children have their parents'.
    for node in world.nodes.values() {
        if let Some(parent) = node.parent
            && let Some(p) = world.nodes.get(&parent)
        {
            assert_eq!(node.policy, p.policy, "node {} did not inherit its policy", node.id);
        }
    }
    let last = frames.last().expect("frames");
    let total = last.policies.selfish + last.policies.always_accept + last.policies.thrifty;
    assert_eq!(total, last.population, "every live node has exactly one policy");
    assert_ne!(
        (first.policies.selfish, first.population),
        (last.policies.selfish, last.population),
        "the free riders' share never moved at all, which cannot be right over 400 ticks"
    );
}

#[test]
fn drift_mixes_the_ways_of_behaving() {
    let cfg = HgtConfig {
        nodes: 32,
        max_nodes: 96,
        epoch_ticks: 200,
        hazard_kinds: 2,
        policy_drift: 0.2,
        hazard_gradient: 0.0,
        ..HgtConfig::default()
    };
    let mut world = World::new(cfg, 2).expect("valid config");
    for _ in 0..300 {
        world.step();
        if world.extinct() {
            break;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for node in world.nodes.values() {
        seen.insert(format!("{:?}", node.policy));
    }
    assert_eq!(seen.len(), 3, "with drift at 0.2 all three policies should be present: {seen:?}");
}

#[test]
fn an_immune_memory_stops_phages_and_costs_the_genes_they_carried() {
    // A phage that hurts a node teaches it to cut that gene on sight. The memory cannot
    // tell a parasite from something useful, so the protection is paid for in transfer.
    let base = HgtConfig {
        nodes: 32,
        max_nodes: 96,
        epoch_ticks: 150,
        hazard_kinds: 3,
        hazard_gradient: 0.0,
        transduction_rate: 0.06,
        lysis_prob: 0.25,
        mechanisms: Mechanisms { conjugation: false, transformation: false, transduction: true },
        ..HgtConfig::default()
    };

    let measure = |crispr_rate: f64| {
        let cfg = HgtConfig { crispr_rate, ..base.clone() };
        let mut world = World::new(cfg, 5).expect("valid config");
        let (mut lysed, mut transduced, mut refused) = (0u32, 0u32, 0u32);
        for _ in 0..600 {
            for e in world.step() {
                match e {
                    Event::Death { cause: hgt::event::Cause::Lysed, .. } => lysed += 1,
                    Event::Acquire { via: Acquisition::Transduction, .. } => transduced += 1,
                    Event::Transfer { refusal: Some(hgt::event::Refusal::Immune), .. } => {
                        refused += 1
                    }
                    _ => {}
                }
            }
            if world.extinct() {
                break;
            }
        }
        (lysed, transduced, refused)
    };

    let (lysed_open, transduced_open, refused_open) = measure(0.0);
    let (lysed_immune, transduced_immune, refused_immune) = measure(1.0);

    assert_eq!(refused_open, 0, "no immune system, no immune refusals");
    assert!(refused_immune > 0, "an immune population should be cutting phage DNA");
    assert!(
        lysed_immune < lysed_open,
        "immunity should reduce phage kills: {lysed_immune} vs {lysed_open}"
    );
    assert!(
        transduced_immune < transduced_open,
        "and it should cost gene flow: {transduced_immune} acquired vs {transduced_open}"
    );
}

#[test]
fn the_two_trees_disagree_exactly_where_a_gene_moved_sideways() {
    let build = |mechanisms: Mechanisms| {
        let cfg = HgtConfig {
            nodes: 24,
            max_nodes: 96,
            epoch_ticks: 150,
            hazard_kinds: 3,
            hazard_gradient: 0.0,
            mechanisms,
            ..HgtConfig::default()
        };
        let seed = 7;
        let mut analyzer = Analyzer::new(&cfg, seed);
        let mut world = World::new(cfg, seed).expect("valid config");
        for e in world.founding_events() {
            analyzer.observe(&e);
        }
        for _ in 0..400 {
            for e in world.step() {
                analyzer.observe(&e);
            }
            if world.extinct() {
                break;
            }
        }
        analyzer
    };

    let bare = build(Mechanisms::none());
    assert!(
        bare.trees().transfers.is_empty(),
        "with no mechanism the transfer graph must be empty: a gene's history is a subtree \
         of the family tree and nothing else"
    );
    assert!(bare.trees().ancestry.len() > 24, "every node that ever lived should be in the tree");

    let full = build(Mechanisms::default());
    let trees = full.trees();
    assert!(!trees.transfers.is_empty(), "transfer is on, so the graphs should disagree somewhere");

    // Every edge in the transfer graph joins two nodes of the family tree, and joins them
    // sideways: a donor is never the recipient's parent at the moment of the gift.
    let born: std::collections::BTreeMap<_, _> =
        trees.ancestry.iter().map(|(n, p, _, _)| (*n, *p)).collect();
    for (_, _, from, to, via) in &trees.transfers {
        assert!(born.contains_key(to), "recipient {to} is not in the family tree");
        assert!(via.is_lateral(), "the transfer graph holds only lateral edges");
        assert_ne!(from, to, "a node cannot give a gene to itself");
    }

    // The exports are the shape other tools expect.
    let newick = trees.newick();
    assert!(newick.lines().all(|l| l.ends_with(';')), "every Newick tree ends in a semicolon");
    for line in newick.lines() {
        let opens = line.chars().filter(|c| *c == '(').count();
        let closes = line.chars().filter(|c| *c == ')').count();
        assert_eq!(opens, closes, "unbalanced Newick: {}", &line[..line.len().min(80)]);
    }
    let tsv = trees.transfers_tsv();
    assert!(tsv.starts_with("tick\tgene\tfrom\tto\tvia\n"));
    assert_eq!(tsv.lines().count(), trees.transfers.len() + 1, "one row per transfer, plus a header");
}

#[test]
fn recombination_builds_a_program_neither_side_had() {
    // Transfer without recombination can only move a gene whole: every improvement has to
    // be walked to by one lineage alone. With it, an arrival is spliced into a resident
    // copy, and what gets integrated is a program that existed nowhere before.
    let cfg = |recombination_rate: f64| HgtConfig {
        nodes: 32,
        max_nodes: 96,
        epoch_ticks: 150,
        hazard_kinds: 3,
        hazard_gradient: 0.0,
        recombination_rate,
        mechanisms: Mechanisms::default(),
        ..HgtConfig::default()
    };

    let run = |recombination_rate: f64| {
        let mut world = World::new(cfg(recombination_rate), 4).expect("valid config");
        let (mut spliced, mut whole) = (0, 0);
        for _ in 0..400 {
            for e in world.step() {
                if let Event::Acquire { spliced: is_splice, .. } = e {
                    if is_splice {
                        spliced += 1;
                    } else {
                        whole += 1;
                    }
                }
            }
            if world.extinct() {
                break;
            }
        }
        (spliced, whole)
    };

    let (off, off_whole) = run(0.0);
    assert_eq!(off, 0, "with recombination off, an arriving gene is kept whole or not at all");
    assert!(off_whole > 0, "genes should still be moving");

    let (on, on_whole) = run(0.5);
    assert!(on > 0, "with it on, transfers should be producing new programs");
    assert!(on_whole > 0, "and half of them should still arrive whole");
}
