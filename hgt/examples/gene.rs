//! Print the compiled resistance gene for each stressor of a run:
//! `cargo run -p hgt --example gene -- [seed]`

use hgt::config::HgtConfig;
use hgt::hazard::Environment;
use hgt::isa::{Instruction, glyphs};

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let cfg = HgtConfig::default();
    let env = Environment::new(&cfg, seed);
    for kind in 0..cfg.hazard_kinds {
        let code = env.resistance_gene(kind);
        let (key, rot) = env.secret(kind);
        println!("stressor {kind}: answer = rotl(payload ^ {key:#010x}, {rot})");
        println!("  {} bytes  {}", code.len(), glyphs(&code));
        let text: Vec<String> =
            code.iter().map(|b| Instruction::decode(*b).to_string()).collect();
        println!("  {}\n", text.join(" "));
    }
}
