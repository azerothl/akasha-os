//! Binaire `aos-sim` : exécute le banc d'essai des 6 scénarios §17.2 et
//! affiche un rapport. Code de sortie 1 si un scénario échoue.

fn main() {
    let reports = aos_sim::run_all();
    let mut failures = 0;
    println!("=== Akasha OS — Banc d'essai P0 (specs-techniques §17.2) ===\n");
    for r in &reports {
        print!("{}", r.render());
        println!();
        if !r.passed() {
            failures += 1;
        }
    }
    let total = reports.len();
    println!("=== {} / {} scénarios passent ===", total - failures, total);
    if failures > 0 {
        std::process::exit(1);
    }
}
