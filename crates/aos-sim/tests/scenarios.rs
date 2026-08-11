//! Test d'intégration P0.4 : les 6 scénarios de `specs-techniques.md` §17.2
//! doivent tous passer (Gate P0, partie « simulation correcte »).

#[test]
fn les_6_scenarios_du_17_2_passent() {
    let reports = aos_sim::run_all();
    let mut msg = String::new();
    for r in &reports {
        if !r.passed() {
            msg.push_str(&r.render());
            msg.push('\n');
        }
    }
    assert!(
        reports.iter().all(|r| r.passed()),
        "scénarios en échec :\n{msg}"
    );
}
