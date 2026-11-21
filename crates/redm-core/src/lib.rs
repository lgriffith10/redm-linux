//! Types de domaine partagés par tous les crates `redm-linux`.
//!
//! Ce crate ne fait aucune E/S : il définit le vocabulaire commun
//! (type d'hôte, constats, classification de résolution, enveloppe de preuve).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Version du schéma des enregistrements de preuve. À incrémenter à chaque
/// changement incompatible du format JSON.
pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// Nature de l'hôte sur lequel une mesure ou un test a été exécuté.
///
/// Toute preuve DOIT porter cette information : un résultat obtenu sous WSL2
/// n'est pas représentatif d'un environnement de jeu Linux natif.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKind {
    NativeLinux,
    Wsl2,
    Wsl1,
    Unknown,
}

impl HostKind {
    /// Seul un Linux natif permet des conclusions de compatibilité finales.
    pub fn is_representative_for_gaming(self) -> bool {
        matches!(self, HostKind::NativeLinux)
    }
}

impl fmt::Display for HostKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            HostKind::NativeLinux => "native-linux",
            HostKind::Wsl2 => "wsl2",
            HostKind::Wsl1 => "wsl1",
            HostKind::Unknown => "unknown",
        })
    }
}

/// Gravité d'un constat. L'ordre est significatif (`Info < … < Blocker`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
    /// Empêche toute progression sans action externe.
    Blocker,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Blocker => "blocker",
        })
    }
}

/// Projet amont auquel un correctif devrait être proposé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpstreamProject {
    Wine,
    Proton,
    Dxvk,
    Vkd3dProton,
    Mesa,
    CitizenFx,
}

/// Qui peut résoudre un constat. Distingue explicitement les problèmes
/// techniques solubles de ceux qui exigent une autorisation de Cfx.re.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Resolution {
    /// Corrigeable par l'utilisateur (paquet, configuration, matériel).
    LocalConfiguration,
    /// Défaut technique à corriger par un patch upstreamable.
    Upstream { project: UpstreamProject },
    /// Ne peut être résolu que par Cfx.re. Ne jamais contourner.
    RequiresCfxAuthorization,
    /// Informations insuffisantes pour classer.
    Unknown,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resolution::LocalConfiguration => f.write_str("configuration locale"),
            Resolution::Upstream { project } => write!(f, "amont ({project:?})"),
            Resolution::RequiresCfxAuthorization => f.write_str("requiert Cfx.re"),
            Resolution::Unknown => f.write_str("inconnue"),
        }
    }
}

/// Un constat produit par une règle de diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub title: String,
    pub resolution: Resolution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<String>,
    /// Nombre d'occurrences (1 pour un constat d'environnement).
    pub occurrences: usize,
    /// Localisation de la première occurrence (ex. `ligne 42`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl Finding {
    pub fn new(
        rule_id: impl Into<String>,
        severity: Severity,
        title: impl Into<String>,
        resolution: Resolution,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity,
            title: title.into(),
            resolution,
            detail: None,
            advice: None,
            occurrences: 1,
            location: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_advice(mut self, advice: impl Into<String>) -> Self {
        self.advice = Some(advice.into());
        self
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

/// Gravité maximale d'une liste de constats (`None` si vide).
pub fn max_severity(findings: &[Finding]) -> Option<Severity> {
    findings.iter().map(|f| f.severity).max()
}

/// Trie par gravité décroissante, puis occurrences décroissantes, puis identifiant.
pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(b.occurrences.cmp(&a.occurrences))
            .then(a.rule_id.cmp(&b.rule_id))
    });
}

/// Enveloppe horodatée et attribuée à un hôte pour toute donnée émise.
///
/// C'est le format d'échange des preuves : sortie JSON de la CLI, artefacts
/// de CI, entrées des futurs agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord<T> {
    pub schema_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub kind: String,
    pub host: HostKind,
    pub collected_at_unix: u64,
    pub payload: T,
}

impl<T> EvidenceRecord<T> {
    pub fn new(kind: impl Into<String>, host: HostKind, payload: T) -> Self {
        Self {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            tool: "redm-linux".to_owned(),
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            kind: kind.into(),
            host,
            collected_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Error < Severity::Blocker);
    }

    #[test]
    fn only_native_linux_is_representative() {
        assert!(HostKind::NativeLinux.is_representative_for_gaming());
        assert!(!HostKind::Wsl2.is_representative_for_gaming());
        assert!(!HostKind::Unknown.is_representative_for_gaming());
    }

    #[test]
    fn sort_and_max() {
        let mut v = vec![
            Finding::new("b", Severity::Info, "b", Resolution::Unknown),
            Finding::new(
                "a",
                Severity::Blocker,
                "a",
                Resolution::RequiresCfxAuthorization,
            ),
            Finding::new("c", Severity::Warning, "c", Resolution::LocalConfiguration),
        ];
        sort_findings(&mut v);
        let ids: Vec<_> = v.iter().map(|f| f.rule_id.as_str()).collect();
        assert_eq!(ids, ["a", "c", "b"]);
        assert_eq!(max_severity(&v), Some(Severity::Blocker));
        assert_eq!(max_severity(&[]), None);
    }

    #[test]
    fn evidence_json_shape() {
        let rec = EvidenceRecord::new("test", HostKind::Wsl2, 42u32);
        let json = serde_json::to_value(&rec).unwrap();
        assert_eq!(json["host"], "wsl2");
        assert_eq!(json["schema_version"], EVIDENCE_SCHEMA_VERSION);
        assert_eq!(json["payload"], 42);
    }

    #[test]
    fn resolution_json_is_tagged() {
        let r = Resolution::Upstream {
            project: UpstreamProject::Wine,
        };
        let json = serde_json::to_value(r).unwrap();
        assert_eq!(json["kind"], "upstream");
        assert_eq!(json["project"], "wine");
    }
}
