//! Sonde d'étalonnage hôte (Gate P0 — validation croisée llama.cpp).
//!
//! Mesure la bande passante RAM réelle de la machine (lecture séquentielle,
//! mono- et multi-thread) pour alimenter un `HardwareProfile` spécifique au
//! hôte, ensuite comparé aux mesures llama.cpp.
//!
//! Usage : `cargo run -p aos-placement --example host_probe --release`

use std::hint::black_box;
use std::thread;
use std::time::Instant;

const SIZE: usize = 2 << 30; // 2 GiB — largement au-delà du cache L3

fn measure_read_bw(buf: &[u64], threads: usize) -> f64 {
    let chunk = buf.len() / threads;
    let start = Instant::now();
    thread::scope(|s| {
        for t in 0..threads {
            let slice = &buf[t * chunk..(t + 1) * chunk];
            s.spawn(move || {
                let mut acc = 0u64;
                for &v in slice {
                    acc = acc.wrapping_add(black_box(v));
                }
                black_box(acc);
            });
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    (threads * chunk * 8) as f64 / elapsed
}

fn main() {
    let buf = vec![0x5A5A_5A5A_5A5A_5A5Au64; SIZE / 8];
    // Warm-up (allocation / first touch).
    black_box(measure_read_bw(&buf, 1));

    for threads in [1, 4, 8] {
        // Meilleur de 3 passages pour lisser le bruit.
        let best = (0..3)
            .map(|_| measure_read_bw(&buf, threads))
            .fold(0.0_f64, f64::max);
        println!("threads={threads:2}  read_bw={:7.2} GB/s", best / 1e9);
    }
}
