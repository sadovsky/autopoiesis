//! Phase 6 acceptance: the gene goes over a real socket.
//!
//! Two demes, two listeners, one localhost network. The second deme is founded with no
//! spare genes at all, so if it is still alive after the stressor shifts, the genes that
//! saved it arrived as bytes on a TCP connection from the first.

use hgt::config::{HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::gene::NodeId;
use hgt::tcp::{ID_STRIDE, TcpTransport, founder_ids, id_base};
use hgt::world::World;
use std::net::SocketAddr;
use std::time::Duration;

fn cfg(founder_carriers: usize) -> HgtConfig {
    HgtConfig {
        nodes: 12,
        max_nodes: 48,
        epoch_ticks: 80,
        hazard_kinds: 3,
        founder_carriers,
        restriction: 0.0,
        mechanisms: Mechanisms::default(),
        ..HgtConfig::default()
    }
}

fn deme(index: u32, addrs: &[SocketAddr], carriers: usize, seed: u64) -> World {
    let base = id_base(index);
    let transport = TcpTransport::bind(index, addrs[index as usize], addrs.to_vec())
        .expect("bind a listener");
    let cfg = cfg(carriers);
    let mut w = World::with_deme(cfg.clone(), seed, Box::new(transport), base..base + ID_STRIDE)
        .expect("valid config");
    let remote: Vec<NodeId> = (0..addrs.len() as u32)
        .filter(|i| *i != index)
        .flat_map(|i| founder_ids(i, cfg.nodes))
        .collect();
    w.introduce(&remote, 2);
    w
}

#[test]
fn a_deme_with_no_spare_genes_survives_on_what_arrives_over_the_wire() {
    let addrs: Vec<SocketAddr> = ["127.0.0.1:19751", "127.0.0.1:19752"]
        .iter()
        .map(|s| s.parse().expect("valid address"))
        .collect();
    let seed = 4;
    let mut rich = deme(0, &addrs, 2, seed);
    let mut poor = deme(1, &addrs, 0, seed);

    let mut crossed = 0;
    for _ in 0..300 {
        rich.step();
        for e in poor.step() {
            if let Event::Acquire { from: Some(donor), node, .. } = e
                && donor / ID_STRIDE != node / ID_STRIDE
            {
                crossed += 1;
            }
        }
        // Real sockets need a moment; the reader threads are what make deliveries visible.
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(crossed > 0, "nothing crossed the socket into the second deme");
    assert!(poor.population() > 0, "the deme with no spare genes died anyway");
    assert!(rich.population() > 0);

    // The control: the same deme, alone in the world, with nobody to receive from.
    let alone_addr: Vec<SocketAddr> = vec!["127.0.0.1:19753".parse().expect("valid address")];
    let mut alone = deme(0, &alone_addr, 0, seed);
    for _ in 0..300 {
        alone.step();
        if alone.extinct() {
            break;
        }
    }
    assert!(alone.extinct(), "a deme with no spares and no peers should not survive the shifts");
}

/// A split network cannot move a gene to the side that needs it, and the population pays
/// for that at the next shift.
#[test]
fn a_partition_costs_the_population_at_the_stressor_it_cannot_answer() {
    use hgt::config::Mechanisms as M;
    use hgt::metrics::{Analyzer, Record};

    let base = |partition: Option<u32>| HgtConfig {
        nodes: 48,
        max_nodes: 256,
        founder_carriers: 1,
        epoch_ticks: 150,
        hazard_kinds: 3,
        hazard_gradient: 0.0,
        partition_at: partition,
        mechanisms: M::default(),
        ..HgtConfig::default()
    };

    let run = |cfg: HgtConfig, seed: u64| {
        let mut analyzer = Analyzer::new(&cfg, seed);
        let mut world = World::new(cfg, seed).expect("valid config");
        for e in world.founding_events() {
            analyzer.observe(&e);
        }
        let (mut min_population, mut cut_frames, mut worst_side) = (usize::MAX, 0, u32::MAX);
        for _ in 0..600 {
            for e in world.step() {
                for rec in analyzer.observe(&e) {
                    if let Record::Frame(f) = rec {
                        if f.partitioned {
                            cut_frames += 1;
                        }
                        worst_side = worst_side.min(f.sides.here_solvers.min(f.sides.there_solvers));
                    }
                }
            }
            min_population = min_population.min(world.population());
            if world.extinct() {
                break;
            }
        }
        (min_population, cut_frames, worst_side)
    };

    for seed in [2u64, 3] {
        let (whole_min, whole_cuts, _) = run(base(None), seed);
        let (cut_min, cut_frames, cut_worst) = run(base(Some(0)), seed);
        assert_eq!(whole_cuts, 0, "no partition was configured, so no frame may report one");
        assert!(cut_frames > 0, "the partitioned run should report itself partitioned");
        assert!(
            cut_min < whole_min,
            "seed {seed}: a split network bottlenecked at {cut_min}, a whole one at {whole_min}"
        );
        assert!(
            cut_worst < 10,
            "seed {seed}: one side should be nearly wiped out of answerers, but the worst \
             it got was {cut_worst}"
        );
    }
}
