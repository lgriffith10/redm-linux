//! Runtimes Wine/Proton et préfixes Wine.
//!
//! Wine et Proton sont regroupés ici : Proton est une distribution de Wine
//! (plus DXVK, VKD3D-Proton et des scripts), et les deux partagent la notion
//! de préfixe. Ce crate ne lance encore rien ; il découvre et inspecte.

pub mod dll;
pub mod graphics;
pub mod registry;
pub mod winpath;

use redm_core::{Finding, Resolution, Severity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Une installation Proton détectée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtonInstall {
    /// Nom du répertoire (ex. `Proton 9.0 (Beta)`, `GE-Proton9-20`).
    pub name: String,
    pub path: PathBuf,
    /// Contenu du fichier `version` (ex. `proton-9.0-4`), si présent.
    pub version: Option<String>,
    /// DXVK et VKD3D-Proton embarqués.
    pub bundled: graphics::BundledTranslators,
}

/// Racines de recherche Proton habituelles pour un `$HOME` donné.
pub fn default_proton_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".steam/steam/steamapps/common"),
        home.join(".local/share/Steam/steamapps/common"),
        home.join(".steam/root/compatibilitytools.d"),
        home.join(".local/share/Steam/compatibilitytools.d"),
        // Steam Flatpak.
        home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/common"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam/compatibilitytools.d"),
        PathBuf::from("/usr/share/steam/compatibilitytools.d"),
    ]
}

/// Le fichier `version` de Proton a la forme `<horodatage> <nom>`.
pub fn parse_proton_version_file(text: &str) -> Option<String> {
    let line = text.lines().next()?.trim();
    let v = match line.split_once(char::is_whitespace) {
        Some((ts, rest)) if ts.chars().all(|c| c.is_ascii_digit()) => rest.trim(),
        _ => line,
    };
    (!v.is_empty()).then(|| v.to_owned())
}

/// Recherche les installations Proton sous les racines données.
///
/// Un répertoire est une installation Proton s'il contient un script `proton`.
/// Les doublons (liens symboliques `~/.steam/steam` → `~/.local/share/Steam`)
/// sont éliminés via le chemin canonique.
pub fn discover_proton(roots: &[PathBuf]) -> Vec<ProtonInstall> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join("proton").is_file() {
                continue;
            }
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let version = fs::read_to_string(path.join("version"))
                .ok()
                .and_then(|t| parse_proton_version_file(&t));
            found.push(ProtonInstall {
                name: entry.file_name().to_string_lossy().into_owned(),
                bundled: graphics::inspect_proton_bundle(&path),
                path,
                version,
            });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Architecture déclarée par un préfixe Wine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrefixArch {
    Win32,
    Win64,
}

/// État d'un préfixe Wine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefixReport {
    pub path: PathBuf,
    pub exists: bool,
    /// `system.reg` et `drive_c` présents.
    pub initialized: bool,
    pub arch: Option<PrefixArch>,
}

/// Inspecte un préfixe Wine (lecture seule).
///
/// Accepte aussi un préfixe Proton (`compatdata/<appid>`) : on descend alors
/// dans `pfx/`.
pub fn inspect_prefix(path: &Path) -> PrefixReport {
    let pfx = if path.join("pfx/system.reg").is_file() {
        path.join("pfx")
    } else {
        path.to_path_buf()
    };
    let system_reg = fs::read_to_string(pfx.join("system.reg")).ok();
    let arch = system_reg.as_deref().and_then(|t| {
        t.lines().take(10).find_map(|l| match l.trim() {
            "#arch=win64" => Some(PrefixArch::Win64),
            "#arch=win32" => Some(PrefixArch::Win32),
            _ => None,
        })
    });
    PrefixReport {
        path: pfx.clone(),
        exists: pfx.is_dir(),
        initialized: system_reg.is_some() && pfx.join("drive_c").is_dir(),
        arch,
    }
}

/// Emplacements de préfixes usuels pour un `$HOME` donné : `~/.wine`,
/// préfixes nommés, Lutris (`~/Games/*`), Steam `compatdata/*` (dont les
/// raccourcis « jeu non-Steam »), Steam Flatpak.
pub fn default_prefix_candidates(home: &Path) -> Vec<PathBuf> {
    let mut v = vec![home.join(".wine")];
    for parent in [
        home.join(".local/share/wineprefixes"),
        home.join("Games"),
        home.join(".steam/steam/steamapps/compatdata"),
        home.join(".local/share/Steam/steamapps/compatdata"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata"),
    ] {
        if let Ok(entries) = fs::read_dir(&parent) {
            v.extend(entries.flatten().map(|e| e.path()));
        }
    }
    v
}

/// Garde les candidats qui sont des préfixes initialisés, sans doublons.
pub fn discover_prefixes(candidates: &[PathBuf]) -> Vec<PrefixReport> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<PrefixReport> = candidates
        .iter()
        .map(|c| inspect_prefix(c))
        .filter(|r| r.initialized)
        .filter(|r| seen.insert(fs::canonicalize(&r.path).unwrap_or_else(|_| r.path.clone())))
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Constats sur les runtimes disponibles.
pub fn assess_runtimes(protons: &[ProtonInstall], system_wine: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    if protons.is_empty() && !system_wine {
        out.push(
            Finding::new(
                "runtime.none",
                Severity::Error,
                "Aucun runtime Windows trouvé (ni Wine système, ni Proton)",
                Resolution::LocalConfiguration,
            )
            .with_advice("Installer Steam + Proton, ou Wine (paquet `wine64` / WineHQ)."),
        );
    }
    out
}

/// Constats sur un préfixe.
pub fn assess_prefix(p: &PrefixReport) -> Vec<Finding> {
    let loc = p.path.display().to_string();
    if !p.exists {
        return vec![
            Finding::new(
                "prefix.missing",
                Severity::Error,
                "Préfixe inexistant",
                Resolution::LocalConfiguration,
            )
            .with_location(loc),
        ];
    }
    let mut out = Vec::new();
    if !p.initialized {
        out.push(
            Finding::new(
                "prefix.uninitialized",
                Severity::Error,
                "Préfixe non initialisé (`system.reg` ou `drive_c` absent)",
                Resolution::LocalConfiguration,
            )
            .with_location(loc.clone())
            .with_advice("Initialiser avec `WINEPREFIX=<chemin> wineboot -i`."),
        );
    }
    if p.arch == Some(PrefixArch::Win32) {
        out.push(
            Finding::new(
                "prefix.win32",
                Severity::Error,
                "Préfixe 32 bits : RDR2 et RedM exigent un préfixe 64 bits",
                Resolution::LocalConfiguration,
            )
            .with_location(loc),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, write};

    #[test]
    fn version_file_parsing() {
        assert_eq!(
            parse_proton_version_file("1718000000 proton-9.0-2\n").as_deref(),
            Some("proton-9.0-2")
        );
        assert_eq!(
            parse_proton_version_file("GE-Proton9-20").as_deref(),
            Some("GE-Proton9-20")
        );
        assert_eq!(parse_proton_version_file(""), None);
    }

    #[test]
    fn discovers_and_dedupes_proton() {
        let tmp = tempfile::tempdir().unwrap();
        let common = tmp.path().join("common");
        let tools = tmp.path().join("compat");
        create_dir_all(common.join("Proton 9.0")).unwrap();
        write(common.join("Proton 9.0/proton"), "#!/usr/bin/env python3\n").unwrap();
        write(
            common.join("Proton 9.0/version"),
            "1718000000 proton-9.0-4\n",
        )
        .unwrap();
        create_dir_all(common.join("NotProton")).unwrap();
        create_dir_all(tools.join("GE-Proton9-20")).unwrap();
        write(tools.join("GE-Proton9-20/proton"), "").unwrap();

        // Même racine passée deux fois : pas de doublon.
        let found = discover_proton(&[common.clone(), tools, common, tmp.path().join("absent")]);
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["GE-Proton9-20", "Proton 9.0"]);
        assert_eq!(found[1].version.as_deref(), Some("proton-9.0-4"));
        assert_eq!(found[0].version, None);
    }

    #[test]
    fn inspects_prefixes() {
        let tmp = tempfile::tempdir().unwrap();

        let missing = inspect_prefix(&tmp.path().join("nope"));
        assert!(!missing.exists);
        assert_eq!(assess_prefix(&missing)[0].rule_id, "prefix.missing");

        let empty = tmp.path().join("empty");
        create_dir_all(&empty).unwrap();
        let r = inspect_prefix(&empty);
        assert!(r.exists && !r.initialized);
        assert_eq!(assess_prefix(&r)[0].rule_id, "prefix.uninitialized");

        // Préfixe Proton : compatdata/<appid>/pfx
        let proton = tmp.path().join("compatdata/123");
        create_dir_all(proton.join("pfx/drive_c")).unwrap();
        write(
            proton.join("pfx/system.reg"),
            "WINE REGISTRY Version 2\n;; All keys relative to \\\\Machine\n\n#arch=win64\n",
        )
        .unwrap();
        let r = inspect_prefix(&proton);
        assert!(r.initialized);
        assert_eq!(r.arch, Some(PrefixArch::Win64));
        assert!(r.path.ends_with("pfx"));
        assert!(assess_prefix(&r).is_empty());
    }

    #[test]
    fn discovers_only_initialized_prefixes() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let mk = |p: &std::path::Path| {
            create_dir_all(p.join("drive_c")).unwrap();
            write(
                p.join("system.reg"),
                "WINE REGISTRY Version 2\n#arch=win64\n",
            )
            .unwrap();
        };
        mk(&home.join(".wine"));
        mk(&home.join("Games/rockstar-games-launcher"));
        mk(&home.join(".local/share/Steam/steamapps/compatdata/3141592653/pfx"));
        create_dir_all(home.join("Games/not-a-prefix")).unwrap();
        let found = discover_prefixes(&default_prefix_candidates(home));
        assert_eq!(found.len(), 3);
        assert!(found.iter().all(|p| p.initialized));
        assert!(found.iter().any(|p| p.path.ends_with("3141592653/pfx")));
    }

    #[test]
    fn no_runtime_is_an_error() {
        assert_eq!(assess_runtimes(&[], false)[0].rule_id, "runtime.none");
        assert!(assess_runtimes(&[], true).is_empty());
    }
}
