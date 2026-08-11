//! Bench criterion de l'algorithme de placement (P0 — dépendance prévue).
//!
//! Mesure le coût de `place_model_with_budgets` sur un modèle 80 couches
//! et le coût d'estimation du modèle de coût.

use aos_placement::{
    CostModel, HardwareProfile, ModelDesc, PlacementManager, PlacementProfile, Priority,
    PrivacyClass,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

const GIB: u64 = 1 << 30;

fn model_32b() -> ModelDesc {
    ModelDesc {
        id: "local:llama-q6-32b".into(),
        name: "Llama 32B Q6".into(),
        n_layers: 80,
        n_params: 32e9,
        weights_bytes: 26 * GIB,
        embed_bytes: 800_000_000,
        kv_bytes_per_token: 400_000,
        context_length: 131072,
        supports_layer_offload: true,
        privacy_class: PrivacyClass::Local,
    }
}

fn bench_placement(c: &mut Criterion) {
    let pm = PlacementManager::new(HardwareProfile::reference_v1(), CostModel::default());
    let m = model_32b();
    c.bench_function("place_model 32B balanced", |b| {
        b.iter(|| {
            pm.place_model(
                black_box(&m),
                black_box(PlacementProfile::Balanced),
                black_box(Priority::AgentNormal),
                black_box(2048),
            )
            .unwrap()
        })
    });
    let plan = pm
        .place_model(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
        .unwrap();
    c.bench_function("estimate 32B", |b| {
        b.iter(|| pm.estimate(black_box(&plan), black_box(&m), 256, 2048))
    });
}

criterion_group!(benches, bench_placement);
criterion_main!(benches);
