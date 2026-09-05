//! Phase 1 acceptance: a gene received from another machine can never hurt the machine
//! that runs it. Random byte strings — which is what a heavily mutated gene is — must
//! terminate inside their budget, without panicking and without reading anything the
//! interpreter was not handed.

use hgt::vm::{Stop, run};
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

#[test]
fn a_hundred_thousand_random_genes_terminate_within_budget() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let budget = 64;
    let mut emitted = 0usize;
    let mut stops = [0usize; 4];
    for _ in 0..100_000 {
        let len = rng.random_range(0..=64usize);
        let code: Vec<u8> = (0..len).map(|_| rng.random::<u8>()).collect();
        let out = run(&code, rng.random::<u32>(), rng.random_range(0..4u8), budget);
        assert!(out.steps <= budget, "gene ran {} steps on a budget of {budget}: {code:?}", out.steps);
        match out.stop {
            Stop::Emitted => {
                stops[0] += 1;
                emitted += 1;
                assert!(out.answer.is_some());
            }
            Stop::Halted => stops[1] += 1,
            Stop::RanOff => stops[2] += 1,
            Stop::Budget => stops[3] += 1,
        }
        if out.stop != Stop::Emitted {
            assert_eq!(out.answer, None, "only Emit produces an answer");
        }
    }
    // Random code answers *something* often enough to be a real search space, and not so
    // often that answering is free: both matter for the sandbox to be interesting.
    assert!(emitted > 1_000, "random genes emitted only {emitted} times: {stops:?}");
    assert!(stops[3] > 0, "no random gene ever hit the step budget: {stops:?}");
}

#[test]
fn a_gene_sees_only_the_stressor_it_is_given() {
    // The interpreter's whole state is acc, aux, payload, kind — there is no memory to
    // carry anything between runs, so the same gene on the same stressor is the same
    // answer, whatever ran before it.
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
    for _ in 0..1_000 {
        let code: Vec<u8> = (0..rng.random_range(1..=32usize)).map(|_| rng.random::<u8>()).collect();
        let payload = rng.random::<u32>();
        let first = run(&code, payload, 1, 96);
        let _noise = run(&code, rng.random::<u32>(), 3, 96);
        assert_eq!(run(&code, payload, 1, 96), first, "a gene's answer depended on history");
    }
}
