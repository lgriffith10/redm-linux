//! Identification des DLL graphiques : Wine intégrée, DXVK, VKD3D-Proton.
//!
//! Marqueurs relevés le 2026-11-21 sur les binaires officiels (voir
//! `docs/references.md`) :
//!
//! - Wine 10.0 : chaîne `Wine builtin DLL` (ou `Wine placeholder DLL`) à
//!   l'offset 0x40, juste après l'en-tête DOS.
//! - DXVK 3.1.1 : séquence `DXVK: \0v3.1.1\0` dans `d3d11.dll`, `dxgi.dll`,
//!   `d3d9.dll` (absente de `d3d8.dll` et `d3d10core.dll`). Le `d3d10core.dll`
//!   de DXVK est un simple relais vers `d3d11.dll`, sans aucun marqueur : il
//!   est classé `OtherPe` (« non Wine »), sans plus de précision.
//! - VKD3D-Proton 3.1 : `vkd3d-proton` dans `d3d12.dll`/`d3d12core.dll`, et
//!   `vkd3d\0…3.1.0\0` (nom et version de moteur Vulkan) dans `d3d12core.dll`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Origine d'une DLL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DllKind {
    WineBuiltin,
    /// DLL fictive créée par Wine quand l'implémentation est absente.
    WinePlaceholder,
    Dxvk,
    Vkd3dProton,
    /// Fichier PE non reconnu (DLL Microsoft native, autre traducteur…).
    OtherPe,
    /// Pas un fichier PE.
    NotPe,
}

/// Une DLL inspectée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DllInfo {
    pub name: String,
    pub path: PathBuf,
    pub kind: DllKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Cible si la DLL est un lien symbolique (Proton lie vers ses fichiers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<PathBuf>,
}

const WINE_HEADER_OFFSET: usize = 0x40;

/// Classe le contenu d'une DLL.
pub fn classify_dll_bytes(bytes: &[u8]) -> (DllKind, Option<String>) {
    if !bytes.starts_with(b"MZ") {
        return (DllKind::NotPe, None);
    }
    let header = bytes
        .get(WINE_HEADER_OFFSET..WINE_HEADER_OFFSET + 20)
        .unwrap_or(&[]);
    if header.starts_with(b"Wine builtin DLL") {
        return (DllKind::WineBuiltin, None);
    }
    if header.starts_with(b"Wine placeholder DLL") {
        return (DllKind::WinePlaceholder, None);
    }
    // DXVK d'abord : ses DLL (interop D3D11On12) mentionnent aussi
    // « vkd3d-proton », alors que VKD3D-Proton ne contient jamais « DXVK ».
    if contains(bytes, b"DXVK") {
        return (DllKind::Dxvk, dxvk_version(bytes));
    }
    if contains(bytes, b"vkd3d-proton") {
        return (DllKind::Vkd3dProton, vkd3d_version(bytes));
    }
    (DllKind::OtherPe, None)
}

/// Inspecte une DLL sur disque. `None` si elle est absente.
pub fn inspect_dll(path: &Path) -> Option<DllInfo> {
    let bytes = fs::read(path).ok()?;
    let (kind, version) = classify_dll_bytes(&bytes);
    Some(DllInfo {
        name: path.file_name()?.to_string_lossy().to_lowercase(),
        path: path.to_path_buf(),
        kind,
        version,
        symlink_target: fs::read_link(path).ok(),
    })
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle, 0).is_some()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Chaîne terminée par NUL commençant à `start`, si imprimable et courte.
fn cstr_at(bytes: &[u8], start: usize, max: usize) -> Option<&str> {
    let rest = bytes.get(start..)?;
    let end = rest.iter().take(max + 1).position(|&b| b == 0)?;
    let s = std::str::from_utf8(&rest[..end]).ok()?;
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic())).then_some(s)
}

fn looks_like_version(s: &str) -> bool {
    let core = s.strip_prefix('v').unwrap_or(s);
    let mut parts = core.split(['.', '-', '+']);
    parts
        .next()
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && core.contains('.')
}

/// DXVK : `DXVK: \0` suivi de la version (`v3.1.1`, `v2.3-12-gabcdef`).
fn dxvk_version(bytes: &[u8]) -> Option<String> {
    let marker = b"DXVK: \0";
    let mut from = 0;
    while let Some(pos) = find(bytes, marker, from) {
        if let Some(s) = cstr_at(bytes, pos + marker.len(), 48)
            && s.starts_with('v')
            && looks_like_version(s)
        {
            return Some(s.to_owned());
        }
        from = pos + 1;
    }
    None
}

/// VKD3D-Proton : `vkd3d\0`, éventuel bourrage NUL, puis `3.1.0\0`.
fn vkd3d_version(bytes: &[u8]) -> Option<String> {
    let marker = b"\0vkd3d\0";
    let mut from = 0;
    while let Some(pos) = find(bytes, marker, from) {
        let mut i = pos + marker.len();
        while bytes.get(i) == Some(&0) && i < pos + marker.len() + 16 {
            i += 1;
        }
        if let Some(s) = cstr_at(bytes, i, 48)
            && looks_like_version(s)
        {
            return Some(s.to_owned());
        }
        from = pos + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// En-tête PE minimal suivi d'une charge utile.
    fn pe(header_at_0x40: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = b"MZ".to_vec();
        v.resize(WINE_HEADER_OFFSET, 0);
        v.extend_from_slice(header_at_0x40);
        v.resize(0x100, 0);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn classifies_wine_builtin_and_placeholder() {
        assert_eq!(
            classify_dll_bytes(&pe(b"Wine builtin DLL", b"")).0,
            DllKind::WineBuiltin
        );
        assert_eq!(
            classify_dll_bytes(&pe(b"Wine placeholder DLL", b"")).0,
            DllKind::WinePlaceholder
        );
        assert_eq!(classify_dll_bytes(b"\x7fELF").0, DllKind::NotPe);
        assert_eq!(
            classify_dll_bytes(&pe(b"This program cannot", b"")).0,
            DllKind::OtherPe
        );
    }

    #[test]
    fn extracts_dxvk_version_with_layout_seen_in_3_1_1() {
        // Octets observés : "\0DXVK: \0v3.1.1\0Build: \0".
        let body = b"DXVK: Failed\0\0DXVK: \0v3.1.1\0Build: \0x";
        assert_eq!(
            classify_dll_bytes(&pe(b"", body)),
            (DllKind::Dxvk, Some("v3.1.1".into()))
        );
        let git = b"\0DXVK: \0v2.3-12-gabcdef0\0";
        assert_eq!(
            classify_dll_bytes(&pe(b"", git)).1.as_deref(),
            Some("v2.3-12-gabcdef0")
        );
        // Régression : DXVK 3.1.1 contient « Device not a vkd3d-proton ».
        let interop = b"Device not a vkd3d-proton\0DXVK: \0v3.1.1\0";
        assert_eq!(classify_dll_bytes(&pe(b"", interop)).0, DllKind::Dxvk);
        // DXVK sans version (d3d8.dll, d3d10core.dll).
        assert_eq!(
            classify_dll_bytes(&pe(b"", b"IDXGIDXVKDevice")),
            (DllKind::Dxvk, None)
        );
    }

    #[test]
    fn extracts_vkd3d_version_with_layout_seen_in_3_1() {
        // Octets observés : "loader.\n\0vkd3d\03.1.0\0\0\0vkd3d-proton - applic".
        let body = b"loader.\n\0vkd3d\x003.1.0\0\0\0vkd3d-proton - applicationVersion";
        assert_eq!(
            classify_dll_bytes(&pe(b"", body)),
            (DllKind::Vkd3dProton, Some("3.1.0".into()))
        );
        // d3d12.dll : marqueur sans version.
        let d3d12 = b"Failed to find vkd3d-proton d3d12core interfaces.";
        assert_eq!(
            classify_dll_bytes(&pe(b"", d3d12)),
            (DllKind::Vkd3dProton, None)
        );
    }

    /// Vérification sur les binaires officiels. Lancer avec :
    /// `REDM_DXVK_DIR=…/dxvk-3.1.1 REDM_VKD3D_DIR=…/vkd3d-proton-3.1 cargo test -- --ignored`
    #[test]
    #[ignore = "nécessite les archives officielles DXVK/VKD3D-Proton"]
    fn real_release_binaries() {
        let dxvk = PathBuf::from(std::env::var("REDM_DXVK_DIR").unwrap());
        for dll in ["d3d11.dll", "dxgi.dll", "d3d9.dll"] {
            for arch in ["x64", "x32"] {
                let i = inspect_dll(&dxvk.join(arch).join(dll)).unwrap();
                assert_eq!(i.kind, DllKind::Dxvk, "{arch}/{dll}");
                assert!(
                    i.version.as_deref().unwrap().starts_with('v'),
                    "{arch}/{dll}"
                );
            }
        }
        let vkd3d = PathBuf::from(std::env::var("REDM_VKD3D_DIR").unwrap());
        for arch in ["x64", "x86"] {
            let core = inspect_dll(&vkd3d.join(arch).join("d3d12core.dll")).unwrap();
            assert_eq!(core.kind, DllKind::Vkd3dProton);
            assert!(core.version.is_some(), "{arch}");
            assert_eq!(
                inspect_dll(&vkd3d.join(arch).join("d3d12.dll"))
                    .unwrap()
                    .kind,
                DllKind::Vkd3dProton
            );
        }
        if let Ok(wine) = std::env::var("REDM_WINE_DLL_DIR") {
            let i = inspect_dll(&PathBuf::from(wine).join("d3d11.dll")).unwrap();
            assert_eq!(i.kind, DllKind::WineBuiltin);
        }
    }
}
