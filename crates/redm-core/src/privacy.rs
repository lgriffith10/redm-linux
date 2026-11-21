//! Anonymisation des données personnelles avant émission d'une preuve.
//!
//! Les preuves sont destinées à être publiées (issues, CI, dépôt). Les
//! journaux CitizenFX contiennent l'adresse e-mail du compte Rockstar et
//! les chemins contiennent les noms d'utilisateur Windows et Linux.

use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Noms d'utilisateur génériques de Wine/Proton, conservés tels quels.
const GENERIC_USERS: &[&str] = &["public", "steamuser", "default", "<user>"];

/// Masque les adresses e-mail et les noms d'utilisateur dans
/// `C:\Users\<nom>`, `drive_c/users/<nom>` et `/home/<nom>`.
///
/// Gère les antislashs doublés (registre Wine, JSON).
pub fn redact(text: &str) -> String {
    static EMAIL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+").unwrap());
    // Windows : le nom peut contenir des espaces (`C:\Users\Jane Doe\`).
    static WIN_USER_DIR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)([a-z]:\\+users\\+)([^\\/"\r\n]+)"#).unwrap());
    static UNIX_USER_DIR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"((?:drive_c/users/)|(?:/home/))([^/\s"]+)"#).unwrap());
    let replace = |c: &Captures| {
        let name = &c[2];
        if GENERIC_USERS.iter().any(|g| g.eq_ignore_ascii_case(name)) {
            c[0].to_owned()
        } else {
            format!("{}<user>", &c[1])
        }
    };
    let s = EMAIL.replace_all(text, "<email>");
    let s = WIN_USER_DIR.replace_all(&s, replace);
    UNIX_USER_DIR.replace_all(&s, replace).into_owned()
}

/// Comme [`redact`], puis masque toute occurrence des noms donnés (typiquement
/// `$USER`), où qu'ils apparaissent : un nom d'utilisateur peut figurer dans
/// des chemins qui n'ont pas la forme `/home/<nom>/`. Les noms de moins de
/// 3 caractères ou génériques sont ignorés pour éviter de masquer du texte
/// ordinaire.
pub fn redact_with(text: &str, names: &[&str]) -> String {
    let mut s = redact(text);
    for name in names {
        let n = name.trim();
        if n.chars().count() < 3 || GENERIC_USERS.iter().any(|g| g.eq_ignore_ascii_case(n)) {
            continue;
        }
        let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(n))).expect("motif échappé");
        s = re.replace_all(&s, "<user>").into_owned();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_emails_and_user_dirs() {
        assert_eq!(
            redact(r#"hello from "C:\Users\Jane Doe\AppData\RedM.exe" as a.b+c@mail.example.org"#),
            r#"hello from "C:\Users\<user>\AppData\RedM.exe" as <email>"#
        );
        assert_eq!(redact("/home/luciano/.wine"), "/home/<user>/.wine");
        assert_eq!(
            redact(r#"{"p":"C:\\users\\bob\\x"}"#),
            r#"{"p":"C:\\users\\<user>\\x"}"#
        );
        assert_eq!(
            redact("pfx/drive_c/users/luciano/AppData"),
            "pfx/drive_c/users/<user>/AppData"
        );
    }

    #[test]
    fn redacts_known_usernames_anywhere() {
        let s = super::redact_with("/tmp/x-home-luciano-projects/pfx and Luciano", &["luciano"]);
        assert_eq!(s, "/tmp/x-home-<user>-projects/pfx and <user>");
        // Noms trop courts ou génériques : ignorés.
        assert_eq!(
            super::redact_with("a bo steamuser", &["bo", "steamuser", ""]),
            "a bo steamuser"
        );
    }

    #[test]
    fn keeps_generic_wine_users_and_is_idempotent() {
        let s = "drive_c/users/steamuser/Documents drive_c/users/Public";
        assert_eq!(redact(s), s);
        let once = redact("/home/bob/x");
        assert_eq!(redact(&once), once);
    }
}
