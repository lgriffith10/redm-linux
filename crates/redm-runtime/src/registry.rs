//! Lecture minimale des fichiers de registre Wine (`system.reg`, `user.reg`).
//!
//! Format (Wine `server/registry.c`) :
//!
//! ```text
//! WINE REGISTRY Version 2
//! #arch=win64
//!
//! [Software\\Wine\\DllOverrides] 1700000000
//! #time=1da...
//! "d3d11"="native,builtin"
//! @="valeur par défaut"
//! "Flag"=dword:00000001
//! "Bin"=hex:01,02,\
//!   03
//! ```
//!
//! Seules les valeurs chaîne (`"…"`, `str(2):"…"`) et `dword` sont décodées.
//! Les recherches de clés et de noms sont insensibles à la casse, comme sous
//! Windows.

use std::collections::HashMap;
use std::path::Path;

/// Valeur de registre décodée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegValue {
    String(String),
    Dword(u32),
    /// Type non décodé (hex, multi-chaîne…).
    Other,
}

/// Un fichier de registre Wine chargé en mémoire.
#[derive(Debug, Default, Clone)]
pub struct RegistryFile {
    /// clé (minuscules, séparateur `\`) → nom (minuscules, `""` = défaut) → valeur
    keys: HashMap<String, HashMap<String, RegValue>>,
}

impl RegistryFile {
    pub fn load(path: &Path) -> Option<Self> {
        std::fs::read_to_string(path).ok().map(|t| Self::parse(&t))
    }

    pub fn parse(text: &str) -> Self {
        let mut keys: HashMap<String, HashMap<String, RegValue>> = HashMap::new();
        let mut current: Option<String> = None;

        for line in text.lines() {
            if let Some(rest) = line.strip_prefix('[') {
                current = rest
                    .rfind(']')
                    .map(|end| unescape(&rest[..end]).to_lowercase());
                if let Some(k) = &current {
                    keys.entry(k.clone()).or_default();
                }
                continue;
            }
            let Some(key) = &current else { continue };
            let Some((name, value)) = parse_value_line(line) else {
                continue;
            };
            keys.entry(key.clone())
                .or_default()
                .insert(name.to_lowercase(), value);
        }
        Self { keys }
    }

    pub fn get(&self, key: &str, name: &str) -> Option<&RegValue> {
        self.keys
            .get(&key.to_lowercase())?
            .get(&name.to_lowercase())
    }

    pub fn get_string(&self, key: &str, name: &str) -> Option<&str> {
        match self.get(key, name)? {
            RegValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Toutes les valeurs chaîne d'une clé (noms en minuscules).
    pub fn strings_in(&self, key: &str) -> Vec<(&str, &str)> {
        let mut v: Vec<_> = self
            .keys
            .get(&key.to_lowercase())
            .into_iter()
            .flatten()
            .filter_map(|(n, val)| match val {
                RegValue::String(s) => Some((n.as_str(), s.as_str())),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v
    }
}

/// `"nom"=valeur` ou `@=valeur`.
fn parse_value_line(line: &str) -> Option<(String, RegValue)> {
    let (name, rest) = if let Some(r) = line.strip_prefix("@=") {
        (String::new(), r)
    } else {
        let r = line.strip_prefix('"')?;
        let end = closing_quote(r)?;
        let name = unescape(&r[..end]);
        (name, r[end + 1..].strip_prefix('=')?)
    };
    let value = if let Some(s) = rest
        .strip_prefix("str(2):")
        .or(Some(rest))
        .and_then(|r| r.strip_prefix('"'))
    {
        let end = closing_quote(s)?;
        RegValue::String(unescape(&s[..end]))
    } else if let Some(d) = rest.strip_prefix("dword:") {
        u32::from_str_radix(d.trim(), 16).map_or(RegValue::Other, RegValue::Dword)
    } else {
        RegValue::Other
    };
    Some((name, value))
}

/// Index du guillemet fermant non échappé.
fn closing_quote(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i),
            _ => {}
        }
    }
    None
}

/// Décode les échappements Wine : `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\xHHHH`.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('x') => {
                let mut hex = String::new();
                while hex.len() < 4 && it.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    hex.push(it.next().unwrap());
                }
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"WINE REGISTRY Version 2
;; All keys relative to \\Machine

#arch=win64

[Software\\Wow6432Node\\Rockstar Games\\Red Dead Redemption 2] 1700000000
#time=1da1234567890ab
"InstallFolder"="C:\\Program Files\\Rockstar Games\\Games\\Red Dead Redemption 2"
"Quoted"="say \"hi\""
"Accent"="caf\xe9"
@="default"
"Flag"=dword:0000002a
"Bin"=hex:01,02,\
  03,04

[Software\\Wine\\DllOverrides] 1700000001
"d3d11"="native,builtin"
"*dxgi"="native"
"#;

    #[test]
    fn parses_strings_dwords_and_escapes() {
        let r = RegistryFile::parse(SAMPLE);
        let key = r"Software\Wow6432Node\Rockstar Games\Red Dead Redemption 2";
        assert_eq!(
            r.get_string(key, "InstallFolder"),
            Some(r"C:\Program Files\Rockstar Games\Games\Red Dead Redemption 2")
        );
        // Insensible à la casse.
        assert!(r.get_string(&key.to_uppercase(), "installfolder").is_some());
        assert_eq!(r.get_string(key, "Quoted"), Some(r#"say "hi""#));
        assert_eq!(r.get_string(key, "Accent"), Some("café"));
        assert_eq!(r.get_string(key, ""), Some("default"));
        assert_eq!(r.get(key, "Flag"), Some(&RegValue::Dword(42)));
        assert_eq!(r.get(key, "Bin"), Some(&RegValue::Other));
        assert_eq!(r.get(key, "Missing"), None);
    }

    #[test]
    fn lists_dll_overrides() {
        let r = RegistryFile::parse(SAMPLE);
        assert_eq!(
            r.strings_in(r"Software\Wine\DllOverrides"),
            [("*dxgi", "native"), ("d3d11", "native,builtin")]
        );
    }
}
