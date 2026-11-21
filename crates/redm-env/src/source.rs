//! Abstraction de l'accès au système, pour rendre la détection testable
//! sans dépendre de l'hôte réel.

use std::path::Path;
use std::process::Command;

/// Résultat d'une commande externe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Accès en lecture seule au système hôte.
pub trait SystemSource {
    fn read_to_string(&self, path: &Path) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    fn env_var(&self, key: &str) -> Option<String>;
    /// `None` si le programme est introuvable ou ne peut pas être lancé.
    fn run(&self, program: &str, args: &[&str]) -> Option<CommandOutput>;
}

/// Implémentation réelle, sur l'hôte courant.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealSystem;

impl SystemSource for RealSystem {
    fn read_to_string(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    fn run(&self, program: &str, args: &[&str]) -> Option<CommandOutput> {
        let out = Command::new(program).args(args).output().ok()?;
        Some(CommandOutput {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Faux système entièrement décrit en mémoire.
    #[derive(Default)]
    pub struct FakeSystem {
        pub files: HashMap<PathBuf, String>,
        pub env: HashMap<String, String>,
        pub commands: HashMap<String, CommandOutput>,
    }

    impl FakeSystem {
        pub fn file(mut self, path: &str, content: &str) -> Self {
            self.files.insert(PathBuf::from(path), content.to_owned());
            self
        }
        pub fn env(mut self, k: &str, v: &str) -> Self {
            self.env.insert(k.to_owned(), v.to_owned());
            self
        }
        pub fn command(mut self, program: &str, stdout: &str) -> Self {
            self.commands.insert(
                program.to_owned(),
                CommandOutput {
                    success: true,
                    stdout: stdout.to_owned(),
                    stderr: String::new(),
                },
            );
            self
        }
    }

    impl SystemSource for FakeSystem {
        fn read_to_string(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
        fn env_var(&self, key: &str) -> Option<String> {
            self.env.get(key).cloned()
        }
        fn run(&self, program: &str, _args: &[&str]) -> Option<CommandOutput> {
            self.commands.get(program).cloned()
        }
    }
}
