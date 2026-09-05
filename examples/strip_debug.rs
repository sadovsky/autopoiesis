use autopoiesis::config::{ExecModel, RepairSource, SimConfig, SunProfile, TilingPattern};
use autopoiesis::grid::{S, TOKEN};
use autopoiesis::isa::Instruction;
use autopoiesis::sim::Sim;
fn main() {
    let cfg = SimConfig { width: 16, height: 16, sun: 1.0, sun_profile: SunProfile::Uniform, noise_rate: 0.0,
        exec_model: ExecModel::Token, token_rate: 0.0, token_init: 0.0, repair_source: RepairSource::Opposite,
        seed_tiling: true, seed_tiling_width: 16, seed_tiling_pattern: TilingPattern::PassThrough, ..SimConfig::default() };
    let reference = Sim::new(cfg.clone(), 3).unwrap().cur.clone();
    let mut sim = Sim::new(cfg, 3).unwrap();
    sim.run(3000);
    let zap = sim.cur.idx(5, 7);
    sim.cur.cells[zap].instr = Instruction::Store(S).encode();
    sim.cur.cells[zap].reg = 0xEE;
    for t in 0..70 {
        sim.step();
        let broken: Vec<String> = (0..256).filter(|&i| sim.cur.cells[i].instr != reference.cells[i].instr)
            .map(|i| format!("({},{})={}", i % 16, i / 16, Instruction::decode(sim.cur.cells[i].instr))).collect();
        let tok: Vec<String> = (0..256).filter(|&i| sim.cur.cells[i].ip & TOKEN != 0).map(|i| format!("({},{})", i%16, i/16)).collect();
        if t < 6 || t % 8 == 0 || (broken.len() > 1 && t < 40) {
            println!("t+{t:2} tokens={} at {:?} broken={} {:?}", tok.len(), &tok[..tok.len().min(3)], broken.len(), &broken[..broken.len().min(6)]);
        }
    }
}
