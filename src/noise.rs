//! Mutation: the entropy the organism fights. Each tick every cell's `instr` is
//! replaced by a random byte with probability `rate`.
//!
//! Implemented with geometric skipping (sample the gap to the next mutated cell) so
//! the cost is proportional to the number of mutations, not the number of cells.

use crate::grid::Grid;
use rand::RngExt;

/// Apply one tick of mutation at `rate`. Returns the number of cells mutated.
pub fn apply<R: RngExt>(grid: &mut Grid, rate: f64, rng: &mut R) -> u64 {
    if rate <= 0.0 {
        return 0;
    }
    let n = grid.len();
    if rate >= 1.0 {
        for c in &mut grid.cells {
            c.instr = rng.random::<u8>();
        }
        return n as u64;
    }
    let log_q = (1.0 - rate).ln();
    let mut count = 0u64;
    // Position of the next mutation: geometric gaps.
    let mut i = skip(rng, log_q);
    while i < n {
        grid.cells[i].instr = rng.random::<u8>();
        count += 1;
        i += 1 + skip(rng, log_q);
    }
    count
}

/// Number of untouched cells before the next hit: floor(ln(U) / ln(1 - p)).
#[inline]
fn skip<R: RngExt>(rng: &mut R, log_q: f64) -> usize {
    let u: f64 = rng.random::<f64>();
    // u in [0, 1); guard the u == 0 edge (ln(0) = -inf) by mapping to a large skip.
    if u <= 0.0 {
        return usize::MAX / 4;
    }
    let g = (u.ln() / log_q).floor();
    if g >= (usize::MAX / 4) as f64 {
        usize::MAX / 4
    } else {
        g as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    #[test]
    fn mutation_rate_matches_expectation() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
        let mut g = Grid::new(100, 100);
        let mut total = 0u64;
        let ticks = 200;
        for _ in 0..ticks {
            total += apply(&mut g, 0.01, &mut rng);
        }
        let expected = 100.0 * 100.0 * 0.01 * ticks as f64; // 20_000
        let dev = (total as f64 - expected).abs() / expected;
        assert!(dev < 0.05, "total {total}, expected {expected}");
        assert_eq!(apply(&mut g, 0.0, &mut rng), 0);
        assert_eq!(apply(&mut g, 1.0, &mut rng), 10_000);
    }
}
