//! Conversion d'un chemin Windows en chemin hôte dans un préfixe Wine.
//!
//! Windows est insensible à la casse, le système de fichiers Linux non :
//! `C:\program files\rockstar games` doit retrouver
//! `drive_c/Program Files/Rockstar Games`. Chaque composant est donc résolu
//! sans tenir compte de la casse.

use std::fs;
use std::path::{Path, PathBuf};

/// Résout `win` (ex. `C:\Program Files\X`) dans le préfixe `pfx`.
///
/// Le lecteur passe par `dosdevices/<x>:` (liens créés par Wine, `z:` → `/`),
/// avec repli sur `drive_c` pour `C:`. Renvoie `None` si le chemin n'est pas
/// absolu ou si le lecteur est inconnu. Le chemin renvoyé peut ne pas exister :
/// les composants introuvables sont conservés tels quels.
pub fn windows_to_host(pfx: &Path, win: &str) -> Option<PathBuf> {
    let win = win.trim().trim_matches('"');
    let mut chars = win.chars();
    let drive = chars.next()?.to_ascii_lowercase();
    if !drive.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let dosdev = pfx.join("dosdevices").join(format!("{drive}:"));
    let base = if dosdev.exists() {
        dosdev
    } else if drive == 'c' && pfx.join("drive_c").is_dir() {
        pfx.join("drive_c")
    } else {
        return None;
    };
    Some(resolve_ci(&base, &win[2..]))
}

/// Joint `rel` (séparateurs `\` ou `/`) à `base` en résolvant chaque
/// composant sans tenir compte de la casse.
pub fn resolve_ci(base: &Path, rel: &str) -> PathBuf {
    let mut cur = base.to_path_buf();
    let mut lost = false;
    for comp in rel
        .split(['\\', '/'])
        .filter(|c| !c.is_empty() && *c != ".")
    {
        if comp == ".." {
            cur.pop();
            continue;
        }
        let exact = cur.join(comp);
        if lost || exact.exists() {
            cur = exact;
            continue;
        }
        let found = fs::read_dir(&cur).ok().and_then(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name())
                .find(|n| n.to_string_lossy().to_lowercase() == comp.to_lowercase())
        });
        match found {
            Some(name) => cur.push(name),
            None => {
                lost = true;
                cur = exact;
            }
        }
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::create_dir_all;

    #[test]
    fn resolves_case_insensitively_via_drive_c() {
        let tmp = tempfile::tempdir().unwrap();
        let pfx = tmp.path();
        create_dir_all(pfx.join("drive_c/Program Files/Rockstar Games/Launcher")).unwrap();
        let p = windows_to_host(pfx, r"c:\program files\ROCKSTAR GAMES\launcher").unwrap();
        assert_eq!(p, pfx.join("drive_c/Program Files/Rockstar Games/Launcher"));
        assert!(p.is_dir());

        // Composant absent : conservé tel quel, sans erreur.
        let missing = windows_to_host(pfx, r"C:\Program Files\Nope\x.exe").unwrap();
        assert_eq!(missing, pfx.join("drive_c/Program Files/Nope/x.exe"));
        assert!(!missing.exists());
    }

    #[test]
    fn uses_dosdevices_and_rejects_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let pfx = tmp.path();
        create_dir_all(pfx.join("dosdevices/d:/Games")).unwrap();
        assert_eq!(
            windows_to_host(pfx, r"D:\games").unwrap(),
            pfx.join("dosdevices/d:/Games")
        );
        assert_eq!(windows_to_host(pfx, r"relative\path"), None);
        assert_eq!(windows_to_host(pfx, r"E:\absent-drive"), None);
    }
}
