//! Lossless grid snapshots for offline analysis and replay.
//!
//! Binary layout (little-endian):
//! ```text
//! magic "APSN" | version u32 | tick u32 | width u32 | height u32 | noise_rate f64
//! | n_cells u32 | n_cells × 6-byte Cell | n_edges u32 | n_edges × (src u32, dst u32)
//! ```
//! The edge list is the deduplicated repair graph over the last `window` ticks, since
//! that is not recoverable from the grid alone.

use crate::grid::{CELL_BYTES, Cell, Grid};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"APSN";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: u32,
    pub noise_rate: f64,
    pub grid: Grid,
    /// Sorted, deduplicated `(source, target)` repair edges over the window.
    pub edges: Vec<(u32, u32)>,
}

impl Snapshot {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.grid.len() * CELL_BYTES + self.edges.len() * 8);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.tick.to_le_bytes());
        out.extend_from_slice(&(self.grid.width as u32).to_le_bytes());
        out.extend_from_slice(&(self.grid.height as u32).to_le_bytes());
        out.extend_from_slice(&self.noise_rate.to_le_bytes());
        out.extend_from_slice(&(self.grid.len() as u32).to_le_bytes());
        for c in &self.grid.cells {
            out.extend_from_slice(&c.to_bytes());
        }
        out.extend_from_slice(&(self.edges.len() as u32).to_le_bytes());
        for &(a, b) in &self.edges {
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        out
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Snapshot> {
        let mut r = Reader { buf, pos: 0 };
        if r.take(4)? != MAGIC {
            bail!("not a snapshot file (bad magic)");
        }
        let version = r.u32()?;
        if version != VERSION {
            bail!("unsupported snapshot version {version}");
        }
        let tick = r.u32()?;
        let width = r.u32()? as usize;
        let height = r.u32()? as usize;
        let noise_rate = r.f64()?;
        let n = r.u32()? as usize;
        if n != width * height {
            bail!("cell count {n} does not match {width}x{height}");
        }
        let mut grid = Grid::new(width, height);
        for c in &mut grid.cells {
            *c = Cell::from_bytes(r.take(CELL_BYTES)?);
        }
        let ne = r.u32()? as usize;
        let mut edges = Vec::with_capacity(ne);
        for _ in 0..ne {
            let a = r.u32()?;
            let b = r.u32()?;
            if a as usize >= n || b as usize >= n {
                bail!("edge ({a}, {b}) out of range");
            }
            edges.push((a, b));
        }
        Ok(Snapshot {
            tick,
            noise_rate,
            grid,
            edges,
        })
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let mut f = fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
        f.write_all(&self.to_bytes())?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Snapshot> {
        let mut buf = Vec::new();
        fs::File::open(path)
            .with_context(|| format!("opening {}", path.display()))?
            .read_to_end(&mut buf)?;
        Snapshot::from_bytes(&buf).with_context(|| format!("decoding {}", path.display()))
    }

    /// `dir/tick_{n}.bin`
    pub fn path_for(dir: &Path, tick: u32) -> PathBuf {
        dir.join(format!("tick_{tick}.bin"))
    }
}

/// All `tick_*.bin` files in `dir`, sorted by tick.
pub fn list_snapshots(dir: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let p = entry?.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(rest) = name.strip_prefix("tick_").and_then(|s| s.strip_suffix(".bin"))
            && let Ok(t) = rest.parse::<u32>()
        {
            out.push((t, p));
        }
    }
    out.sort();
    Ok(out)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            bail!("truncated snapshot at byte {}", self.pos);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64(&mut self) -> Result<f64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    #[test]
    fn bytes_roundtrip_losslessly() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(9);
        let grid = Grid::random(7, 5, 900, &mut rng);
        let snap = Snapshot {
            tick: 4242,
            noise_rate: 0.0125,
            grid,
            edges: vec![(0, 1), (3, 34), (34, 3)],
        };
        let back = Snapshot::from_bytes(&snap.to_bytes()).unwrap();
        assert_eq!(back, snap);
        assert!(Snapshot::from_bytes(&snap.to_bytes()[..40]).is_err());
        assert!(Snapshot::from_bytes(b"nope").is_err());
    }
}
