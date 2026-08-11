//! Les 6 scénarios obligatoires de `specs-techniques.md` §17.2.

use crate::{Check, ScenarioReport};
use aos_placement::{
    Bound, CostModel, HardwareProfile, ModelDesc, PlacementProfile, PlacementSim, Priority,
    SimEvent, Tier,
};
use aos_registry::{FakeLocalBackend, InferRequest, ModelRegistry, SimBackend};

const CATALOG: &str = include_str!("../../../data/models/catalog.yaml");

fn registry() -> ModelRegistry {
    ModelRegistry::from_yaml(CATALOG).expect("catalogue YAML valide")
}

fn model(id: &str) -> ModelDesc {
    registry().get(id).unwrap().to_model_desc().unwrap()
}

fn ref_sim() -> PlacementSim {
    PlacementSim::new(HardwareProfile::reference_v1(), CostModel::default())
}

fn fmt_est(tok_s: f64, ttft_ms: f64) -> String {
    format!("{tok_s:6.2} tok/s | TTFT {ttft_ms:7.1} ms")
}

/// S1 — Modèle < VRAM → full GPU.
pub fn s1_full_gpu() -> ScenarioReport {
    let mut r = ScenarioReport::new("S1", "Modèle < VRAM → full GPU");
    let mut sim = ref_sim();
    let m = model("local:embedded-instruct");
    sim.place(&m, PlacementProfile::Latency, Priority::Interactive, 2048)
        .expect("placement S1");
    let pm = sim.get(&m.id).unwrap();
    let plan = &pm.plan;

    r.checks.push(Check::expect(
        "toutes les couches en VRAM",
        plan.layer_bytes_on(Tier::Vram)
            + plan.layer_bytes_on(Tier::Ram)
            + plan.layer_bytes_on(Tier::Disk)
            == plan.layer_bytes_on(Tier::Vram),
        plan.summary(),
    ));
    r.checks.push(Check::expect(
        "KV cache en VRAM",
        plan.kv_bytes_on(Tier::Vram) > 0,
        format!("KV VRAM = {} o", plan.kv_bytes_on(Tier::Vram)),
    ));
    r.checks.push(Check::expect(
        "aucun octet sur disque",
        plan.bytes_on(Tier::Disk) == 0,
        format!("DISK = {} o", plan.bytes_on(Tier::Disk)),
    ));

    let est = sim.estimate(&m.id, 256, 2048).unwrap();
    r.notes.push(format!(
        "embedded-instruct warm : {}",
        fmt_est(est.tok_s, est.ttft_ms)
    ));
    r.checks.push(Check::expect(
        "TTFT < 2 s warm (NFR-01 / Gate P1 anticipé)",
        est.ttft_ms < 2000.0,
        format!("{:.1} ms", est.ttft_ms),
    ));
    r.checks.push(Check::expect(
        "débit embedded plausible (> 20 tok/s)",
        est.tok_s > 20.0,
        format!("{:.1} tok/s", est.tok_s),
    ));
    r
}

/// S2 — VRAM < modèle < RAM → hybride GPU+RAM.
pub fn s2_hybrid_gpu_ram() -> ScenarioReport {
    let mut r = ScenarioReport::new("S2", "VRAM < modèle < RAM → hybride GPU+RAM");
    let mut sim = ref_sim();
    let m = model("local:llama-q6-32b");
    sim.place(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
        .expect("placement S2");
    let plan = &sim.get(&m.id).unwrap().plan;

    r.checks.push(Check::expect(
        "couches en VRAM > 0",
        plan.n_layers_on(Tier::Vram) > 0,
        format!("{} couches", plan.n_layers_on(Tier::Vram)),
    ));
    r.checks.push(Check::expect(
        "couches en RAM > 0",
        plan.n_layers_on(Tier::Ram) > 0,
        format!("{} couches", plan.n_layers_on(Tier::Ram)),
    ));
    r.checks.push(Check::expect(
        "aucune couche sur DISK (modèle < RAM)",
        plan.n_layers_on(Tier::Disk) == 0,
        format!("{} couches", plan.n_layers_on(Tier::Disk)),
    ));

    let est = sim.estimate(&m.id, 256, 2048).unwrap();
    r.notes.push(format!(
        "32B Q6 hybride : {}",
        fmt_est(est.tok_s, est.ttft_ms)
    ));
    r.notes.push(format!("plan : {}", plan.summary()));
    r.checks.push(Check::expect(
        "plan viable et plage hybride plausible (0,5–50 tok/s)",
        est.tok_s > 0.5 && est.tok_s < 50.0,
        format!("{:.2} tok/s", est.tok_s),
    ));
    r
}

/// S3 — Modèle > RAM → GPU+RAM+DISK streaming.
///
/// Vérifie aussi la cible §13 : dégradation sous offload disque
/// ≥ 25 % du tok/s full-RAM (même machine, RAM fictivement illimitée).
pub fn s3_disk_streaming() -> ScenarioReport {
    let mut r = ScenarioReport::new("S3", "Modèle > RAM → GPU+RAM+DISK streaming");
    let mut sim = ref_sim();
    let m = model("local:llama-q4-70b");
    sim.place(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
        .expect("placement S3");
    let plan = &sim.get(&m.id).unwrap().plan;

    r.checks.push(Check::expect(
        "les 3 tiers sont utilisés",
        plan.n_layers_on(Tier::Vram) > 0
            && plan.n_layers_on(Tier::Ram) > 0
            && plan.n_layers_on(Tier::Disk) > 0,
        plan.summary(),
    ));

    let est = sim.estimate(&m.id, 256, 2048).unwrap();
    r.notes.push(format!(
        "70B Q4 streaming : {}",
        fmt_est(est.tok_s, est.ttft_ms)
    ));
    r.checks.push(Check::expect(
        "plan viable malgré le streaming (seuil §3.5.3)",
        est.viable,
        format!("{:.2} tok/s", est.tok_s),
    ));

    // Baseline full-RAM : même machine, RAM fictivement illimitée, cpu-only
    // (toutes les couches en RAM, calculées CPU comme dans llama.cpp).
    let mut big = HardwareProfile::reference_v1();
    big.ram_total = 1 << 40; // 1 TiB
    let mut sim_big = PlacementSim::new(big, CostModel::default());
    sim_big
        .place(&m, PlacementProfile::CpuOnly, Priority::AgentNormal, 2048)
        .expect("baseline full-RAM");
    let est_full = sim_big.estimate(&m.id, 256, 2048).unwrap();
    let ratio = est.tok_s / est_full.tok_s;
    r.notes.push(format!(
        "baseline full-RAM : {:.2} tok/s → ratio streaming/full-RAM = {:.2}",
        est_full.tok_s, ratio
    ));
    r.checks.push(Check::expect(
        "dégradation sous offload disque ≥ 25 % du full-RAM (§13)",
        ratio >= 0.25,
        format!("ratio = {ratio:.2}"),
    ));
    r.checks.push(Check::expect(
        "le goulot est bien le streaming disque",
        est.decode_bound == Bound::Stream,
        format!("bound = {:?}", est.decode_bound),
    ));
    r
}

/// S4 — 2 modèles concurrents → éviction fair/priority.
pub fn s4_concurrent_eviction() -> ScenarioReport {
    let mut r = ScenarioReport::new("S4", "2 modèles concurrents → éviction fair/priority");
    let mut sim = ref_sim();
    let m_a = model("local:llama-q6-32b");
    let mut m_b = m_a.clone();
    m_b.id = "local:llama-q6-32b-bis".into();

    sim.place(
        &m_a,
        PlacementProfile::Balanced,
        Priority::AgentNormal,
        2048,
    )
    .expect("placement A");
    let vram_a_before = sim.get(&m_a.id).unwrap().plan.bytes_on(Tier::Vram);
    sim.touch(&m_a.id); // A a tourné récemment

    sim.place(&m_b, PlacementProfile::Latency, Priority::AgentHigh, 2048)
        .expect("placement B (éviction attendue)");

    let vram_a_after = sim.get(&m_a.id).unwrap().plan.bytes_on(Tier::Vram);
    let vram_b = sim.get(&m_b.id).unwrap().plan.bytes_on(Tier::Vram);

    let migrated_from_a = sim.events().iter().any(
        |e| matches!(e, SimEvent::Migrated { model, from: Tier::Vram, .. } if *model == m_a.id),
    );
    r.checks.push(Check::expect(
        "des shards VRAM du modèle A (moins prioritaire) ont migré",
        migrated_from_a && vram_a_after < vram_a_before,
        format!("VRAM A : {} → {} o", vram_a_before, vram_a_after),
    ));
    r.checks.push(Check::expect(
        "le modèle B (haute priorité) a obtenu de la VRAM",
        vram_b > 0,
        format!("VRAM B = {vram_b} o"),
    ));
    r.checks.push(Check::expect(
        "les deux modèles restent placés et actifs",
        sim.get(&m_a.id).is_some() && sim.get(&m_b.id).is_some(),
        "A et B dans le simulateur",
    ));
    // Fairness : aucun shard épinglé migré, aucun modèle plus prioritaire touché.
    let touched_b = sim
        .events()
        .iter()
        .any(|e| matches!(e, SimEvent::Migrated { model, .. } if *model == m_b.id));
    r.checks.push(Check::expect(
        "jamais d'éviction sur le modèle le plus prioritaire (F-PLC-06)",
        !touched_b,
        "aucun événement Migrated sur B",
    ));
    for (id, est) in [
        (&m_a.id, sim.estimate(&m_a.id, 256, 2048).unwrap()),
        (&m_b.id, sim.estimate(&m_b.id, 256, 2048).unwrap()),
    ] {
        r.notes
            .push(format!("{id} : {}", fmt_est(est.tok_s, est.ttft_ms)));
    }
    r
}

/// S5 — Passage latency → memory-saver à chaud.
pub fn s5_hot_reprofile() -> ScenarioReport {
    let mut r = ScenarioReport::new("S5", "Passage latency → memory-saver à chaud");
    let mut sim = ref_sim();
    let m = model("local:llama-q6-32b");
    sim.place(&m, PlacementProfile::Latency, Priority::AgentNormal, 2048)
        .expect("placement S5");
    let before = sim.get(&m.id).unwrap().plan.clone();

    let rep = sim
        .reprofile(&m.id, PlacementProfile::MemorySaver)
        .expect("reprofilage S5");

    r.checks.push(Check::expect(
        "le passage de profil ne migre que des shards (pas de rechargement)",
        rep.migrated_bytes > 0 && rep.migrated_bytes < m.weights_bytes,
        format!(
            "{:.2} GiB migrés sur {:.2} GiB de poids",
            rep.migrated_bytes as f64 / (1u64 << 30) as f64,
            m.weights_bytes as f64 / (1u64 << 30) as f64
        ),
    ));
    r.checks.push(Check::expect(
        "VRAM réduite après memory-saver",
        rep.plan.bytes_on(Tier::Vram) < before.bytes_on(Tier::Vram),
        format!(
            "VRAM {} → {} o",
            before.bytes_on(Tier::Vram),
            rep.plan.bytes_on(Tier::Vram)
        ),
    ));
    r.checks.push(Check::expect(
        "KV ramené au micro-batch (§3.5.6)",
        rep.plan.kv_tokens <= aos_placement::manager::MICRO_BATCH_KV_TOKENS,
        format!("kv_tokens = {}", rep.plan.kv_tokens),
    ));
    let est = sim.estimate(&m.id, 256, rep.plan.kv_tokens).unwrap();
    r.notes.push(format!(
        "après reprofilage : {} | bascule estimée {:.0} ms",
        fmt_est(est.tok_s, est.ttft_ms),
        rep.est_switch_ms
    ));
    r.checks.push(Check::expect(
        "le modèle reste utilisable après bascule (plan viable)",
        est.viable,
        format!("{:.2} tok/s", est.tok_s),
    ));
    r
}

/// S6 — Arrivée d'un agent haute priorité pendant une inférence batch.
pub fn s6_priority_pressure() -> ScenarioReport {
    let mut r = ScenarioReport::new("S6", "Agent haute priorité pendant inférence batch");
    let mut sim = ref_sim();
    let m_big = model("local:llama-q6-32b");
    let m_small = model("local:embedded-instruct");

    // Deux inférences en cours : une interactive normale, une batch.
    sim.place(
        &m_big,
        PlacementProfile::Balanced,
        Priority::AgentNormal,
        2048,
    )
    .expect("placement big");
    sim.place(&m_small, PlacementProfile::Latency, Priority::Batch, 2048)
        .expect("placement small");

    // Le batch tourne (backend simulé) quand l'agent critique arrive.
    let gen = {
        let be = FakeLocalBackend::new(&sim);
        be.infer(&InferRequest {
            request_id: 1,
            model_id: m_small.id.clone(),
            prompt_tokens: 128,
            max_output_tokens: 32,
            ctx_tokens: 1024,
        })
        .expect("inférence batch simulée")
    };
    r.notes.push(format!(
        "batch en cours : {}",
        fmt_est(gen.tok_s, gen.ttft_ms)
    ));

    // Besoin VRAM d'un agent SystemCritical (ex. assistant système + modèle).
    let need = 4u64 << 30;
    let rep = sim.handle_pressure(need, Priority::SystemCritical);
    r.checks.push(Check::expect(
        "pression résolue pour l'agent critique",
        rep.satisfied,
        format!("actions : {}", rep.actions.join(" ; ")),
    ));
    r.checks.push(Check::expect(
        "l'échelle §3.5.5 a été suivie (batch réduit et/ou migration/suspension)",
        sim.events().iter().any(|e| {
            matches!(
                e,
                SimEvent::BatchReduced { .. }
                    | SimEvent::Migrated { .. }
                    | SimEvent::Suspended { .. }
            )
        }),
        format!("{} événements", sim.events().len()),
    ));
    r.checks.push(Check::expect(
        "seuls des modèles moins prioritaires ont été touchés",
        sim.events().iter().all(|e| match e {
            SimEvent::Suspended { model } | SimEvent::BatchReduced { model } => {
                *model == m_big.id || *model == m_small.id
            }
            _ => true,
        }),
        "aucun effet de bord extérieur",
    ));

    // L'agent critique peut placer son modèle immédiatement après.
    let mut m_hp = m_small.clone();
    m_hp.id = "local:embedded-instruct-hp".into();
    let placed = sim.place(
        &m_hp,
        PlacementProfile::Latency,
        Priority::SystemCritical,
        2048,
    );
    r.checks.push(Check::expect(
        "le modèle de l'agent critique se place après la pression",
        placed.is_ok(),
        match &placed {
            Ok(()) => "placement OK".to_string(),
            Err(e) => e.to_string(),
        },
    ));
    r
}
