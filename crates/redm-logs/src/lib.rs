//! Analyse de journaux à base de règles.
//!
//! Chaque règle associe une expression régulière à une gravité et à une
//! classification de résolution. Les occurrences sont regroupées par
//! (règle, première capture) : 300 lignes `MSVCP140.dll not found`
//! donnent un seul constat avec `occurrences = 300`.
//!
//! Les règles ne décrivent que des messages génériques et documentés de
//! Wine/Vulkan. Les signatures propres à CitizenFX seront ajoutées à partir
//! de journaux réels versionnés comme preuves (voir `docs/policy.md`).

use redm_core::{Finding, Resolution, Severity, UpstreamProject, sort_findings};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::sync::LazyLock;

/// Définition statique d'une règle.
#[derive(Debug, Clone, Copy)]
pub struct RuleDef {
    pub id: &'static str,
    pub severity: Severity,
    pub resolution: Resolution,
    pub title: &'static str,
    /// Expression régulière ; le premier groupe capturant sert à regrouper.
    pub pattern: &'static str,
    pub advice: Option<&'static str>,
}

const WINE: Resolution = Resolution::Upstream {
    project: UpstreamProject::Wine,
};

/// Règles intégrées.
pub const BUILTIN_RULES: &[RuleDef] = &[
    RuleDef {
        id: "anticheat.adhesive",
        severity: Severity::Blocker,
        resolution: Resolution::RequiresCfxAuthorization,
        title: "Message lié à l'anti-triche Adhesive",
        pattern: r"(?i)\badhesive\b",
        advice: Some(
            "Ne pas contourner, désactiver ni usurper Adhesive. Documenter le \
             comportement observé et le remonter à Cfx.re.",
        ),
    },
    RuleDef {
        id: "wine.missing-dll",
        severity: Severity::Error,
        resolution: Resolution::LocalConfiguration,
        title: "DLL importée introuvable",
        pattern: r"err:module:import_dll Library (\S+) \(which is needed by",
        advice: Some(
            "Installer le redistribuable correspondant (ex. VC++ via winetricks) ou utiliser Proton.",
        ),
    },
    RuleDef {
        id: "wine.dll-not-found-status",
        severity: Severity::Error,
        resolution: Resolution::LocalConfiguration,
        title: "Chargement interrompu : STATUS_DLL_NOT_FOUND (c0000135)",
        pattern: r"(?i)status c0000135",
        advice: None,
    },
    RuleDef {
        id: "wine.unimplemented-function",
        severity: Severity::Error,
        resolution: WINE,
        title: "Appel à une fonction non implémentée par Wine",
        pattern: r"(?i)unimplemented function (\S+?),? aborting",
        advice: Some(
            "Candidat à un patch Wine upstream : ajouter un test de conformance puis l'implémentation.",
        ),
    },
    RuleDef {
        id: "wine.crash",
        severity: Severity::Error,
        resolution: Resolution::Unknown,
        title: "Exception non gérée / page fault",
        pattern: r"(?i)unhandled (page fault|exception)",
        advice: Some(
            "Capturer la trace complète avec `WINEDEBUG=+seh,+loaddll` et la versionner comme preuve.",
        ),
    },
    RuleDef {
        id: "vulkan.error",
        severity: Severity::Error,
        resolution: Resolution::Unknown,
        title: "Erreur Vulkan",
        pattern: r"(VK_ERROR_[A-Z_]+)",
        advice: Some("Vérifier `vulkaninfo --summary` et la version de Mesa."),
    },
    RuleDef {
        id: "wine.ntlm-auth-missing",
        severity: Severity::Warning,
        resolution: Resolution::LocalConfiguration,
        title: "`ntlm_auth` introuvable",
        pattern: r"ntlm_auth was not found",
        advice: Some("Installer `winbind` (Debian/Ubuntu)."),
    },
    RuleDef {
        id: "wine.stub",
        severity: Severity::Info,
        resolution: WINE,
        title: "Fonction Wine à l'état de stub",
        pattern: r"fixme:[\w-]+:(\S+) stub",
        advice: None,
    },
];

static COMPILED: LazyLock<Vec<(RuleDef, Regex)>> = LazyLock::new(|| {
    BUILTIN_RULES
        .iter()
        .map(|r| (*r, Regex::new(r.pattern).expect("règle intégrée invalide")))
        .collect()
});

/// Résultat de l'analyse d'un journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAnalysis {
    pub lines_scanned: usize,
    /// Nombre total de lignes `fixme:` (bruit Wine, utile en tendance).
    pub fixme_lines: usize,
    pub findings: Vec<Finding>,
}

#[derive(Default)]
struct Acc {
    count: usize,
    first_line: usize,
}

/// Analyse un flux ligne par ligne. Tolère l'UTF-8 invalide (fréquent dans
/// les journaux Wine).
pub fn analyze<R: BufRead>(mut reader: R) -> io::Result<LogAnalysis> {
    let rules = &*COMPILED;
    let mut groups: BTreeMap<(usize, Option<String>), Acc> = BTreeMap::new();
    let mut buf = Vec::new();
    let mut lines_scanned = 0;
    let mut fixme_lines = 0;

    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        lines_scanned += 1;
        let line = String::from_utf8_lossy(&buf);
        if line.contains("fixme:") {
            fixme_lines += 1;
        }
        for (idx, (_, re)) in rules.iter().enumerate() {
            if let Some(caps) = re.captures(&line) {
                let key = caps.get(1).map(|m| m.as_str().to_owned());
                let acc = groups.entry((idx, key)).or_default();
                if acc.count == 0 {
                    acc.first_line = lines_scanned;
                }
                acc.count += 1;
            }
        }
    }

    let mut findings: Vec<Finding> = groups
        .into_iter()
        .map(|((idx, key), acc)| {
            let def = &rules[idx].0;
            let mut f = Finding::new(def.id, def.severity, def.title, def.resolution)
                .with_location(format!("ligne {}", acc.first_line));
            f.occurrences = acc.count;
            f.detail = key;
            f.advice = def.advice.map(str::to_owned);
            f
        })
        .collect();
    sort_findings(&mut findings);

    Ok(LogAnalysis {
        lines_scanned,
        fixme_lines,
        findings,
    })
}

/// Raccourci pour une chaîne en mémoire.
pub fn analyze_str(text: &str) -> LogAnalysis {
    analyze(text.as_bytes()).expect("lecture en mémoire infaillible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_rules_compile_and_ids_are_unique() {
        assert_eq!(COMPILED.len(), BUILTIN_RULES.len());
        let mut ids: Vec<_> = BUILTIN_RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUILTIN_RULES.len());
    }

    #[test]
    fn groups_missing_dlls_by_name() {
        let log = "\
0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\a.exe\") not found
0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\b.dll\") not found
0024:err:module:import_dll Library VCRUNTIME140.dll (which is needed by L\"C:\\\\a.exe\") not found
";
        let a = analyze_str(log);
        assert_eq!(a.lines_scanned, 3);
        let dll: Vec<_> = a
            .findings
            .iter()
            .filter(|f| f.rule_id == "wine.missing-dll")
            .collect();
        assert_eq!(dll.len(), 2);
        assert_eq!(dll[0].detail.as_deref(), Some("MSVCP140.dll"));
        assert_eq!(dll[0].occurrences, 2);
        assert_eq!(dll[0].location.as_deref(), Some("ligne 1"));
    }

    #[test]
    fn adhesive_is_a_blocker_requiring_cfx() {
        let a = analyze_str("noise\n[  1234] Adhesive: something happened\n");
        let f = &a.findings[0];
        assert_eq!(f.rule_id, "anticheat.adhesive");
        assert_eq!(f.severity, Severity::Blocker);
        assert_eq!(f.resolution, Resolution::RequiresCfxAuthorization);
        assert_eq!(f.location.as_deref(), Some("ligne 2"));
    }

    #[test]
    fn detects_crash_vulkan_and_unimplemented() {
        let log = "\
wine: Unhandled page fault on read access to 0000000000000000 at address 0000000140001000
wine: Call from 00006FFFFFC3A1B2 to unimplemented function api-ms-win-core-foo-l1-1-0.dll.Bar, aborting
err:   vkCreateDevice failed: VK_ERROR_INITIALIZATION_FAILED
0090:fixme:ntdll:NtQuerySystemInformation info_class SYSTEM_PERFORMANCE_INFORMATION
0090:fixme:kernelbase:AppPolicyGetProcessTerminationMethod stub
";
        let a = analyze_str(log);
        let get = |id: &str| a.findings.iter().find(|f| f.rule_id == id).cloned();
        assert_eq!(
            get("wine.crash").unwrap().detail.as_deref(),
            Some("page fault")
        );
        assert_eq!(
            get("wine.unimplemented-function")
                .unwrap()
                .detail
                .as_deref(),
            Some("api-ms-win-core-foo-l1-1-0.dll.Bar")
        );
        assert_eq!(
            get("vulkan.error").unwrap().detail.as_deref(),
            Some("VK_ERROR_INITIALIZATION_FAILED")
        );
        assert_eq!(
            get("wine.stub").unwrap().detail.as_deref(),
            Some("AppPolicyGetProcessTerminationMethod")
        );
        assert_eq!(a.fixme_lines, 2);
        // Tri : les erreurs avant l'info.
        assert_eq!(a.findings.last().unwrap().severity, Severity::Info);
    }

    #[test]
    fn tolerates_invalid_utf8_and_empty_input() {
        let bytes: &[u8] = b"\xff\xfe garbage\nntlm_auth was not found\n";
        let a = analyze(bytes).unwrap();
        assert_eq!(a.lines_scanned, 2);
        assert_eq!(a.findings[0].rule_id, "wine.ntlm-auth-missing");

        let empty = analyze_str("");
        assert_eq!(empty.lines_scanned, 0);
        assert!(empty.findings.is_empty());
    }
}
