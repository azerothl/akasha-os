//! Exemple : validation croisée du modèle de coût vs mesures llama.cpp.
//!
//! `cargo run -p aos-sim --example xval`

fn main() {
    let rows = aos_sim::xval::cross_validate();
    println!("=== Validation croisée P0 — simulateur vs llama.cpp b10361 ===");
    println!("(hôte : Ryzen 7 9800X3D, DDR5 mesurée 45,2 GB/s, CPU-only)\n");
    print!("{}", aos_sim::xval::render_rows(&rows));
    let ok = rows.iter().all(|r| r.error_calibrated_pct.abs() < 30.0);
    println!(
        "\nGate P0 (< 30 % après étalonnage) : {}",
        if ok { "PASS" } else { "FAIL" }
    );
    if !ok {
        std::process::exit(1);
    }
}
