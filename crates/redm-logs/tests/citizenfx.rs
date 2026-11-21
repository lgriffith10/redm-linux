//! Tests sur un extrait de journal CitizenFX/RedM réel (source publique,
//! anonymisée, voir `docs/references.md`).

use redm_core::Severity;
use redm_logs::analyze_str;

const EXCERPT: &str = include_str!("fixtures/citizenfx-redm-excerpt.log");

#[test]
fn normal_adhesive_component_load_is_not_a_blocker() {
    let a = analyze_str(EXCERPT);
    assert!(
        a.findings.iter().all(|f| f.severity != Severity::Blocker),
        "faux positif Adhesive : {:#?}",
        a.findings
    );
    let comp = a
        .findings
        .iter()
        .find(|f| f.rule_id == "cfx.adhesive-component")
        .expect("chargement du composant détecté");
    assert_eq!(comp.severity, Severity::Info);
}

#[test]
fn extracts_citizenfx_facts() {
    let f = analyze_str(EXCERPT).citizenfx;
    assert!(f.is_citizenfx_log);
    assert_eq!(f.game_build, Some(1491));
    assert_eq!(f.game_version.as_deref(), Some("1.0.1491.18"));
    assert_eq!(
        f.game_install_location.as_deref(),
        Some(r"C:\Program Files\Rockstar Games\Games\Red Dead Redemption 2")
    );
}

#[test]
fn detects_crash_and_faulting_module() {
    let a = analyze_str(EXCERPT);
    assert!(a.findings.iter().any(|f| f.rule_id == "cfx.crash"));
    let loc = a
        .findings
        .iter()
        .find(|f| f.rule_id == "cfx.crash-location")
        .unwrap();
    assert_eq!(loc.detail.as_deref(), Some("d3d11.dll"));
}

#[test]
fn no_personal_data_leaks_into_output() {
    let json = serde_json::to_string(&analyze_str(EXCERPT)).unwrap();
    assert!(!json.contains("jane.doe@example.com"));
    assert!(!json.contains("Jane Doe"));
}
