//! `redm-linux` : point d'entrée de la plateforme.
//!
//! Codes de sortie :
//! - 0 : aucun constat de gravité ≥ error
//! - 1 : échec de l'outil lui-même (E/S, arguments…)
//! - 2 : au moins un constat `error`
//! - 3 : au moins un constat `blocker`

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use redm_core::{EvidenceRecord, Finding, HostKind, Severity, max_severity};
use redm_env::{RealSystem, SystemSource};
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "redm-linux",
    version,
    about = "Plateforme d'ingénierie de compatibilité RedM ↔ Linux"
)]
struct Cli {
    /// Sortie JSON (enveloppe de preuve versionnée) au lieu du texte.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Décrit l'hôte : WSL2/natif, CPU, mémoire, Vulkan, Wine.
    Env,
    /// Liste les installations Proton détectées.
    Runtimes {
        /// Racine supplémentaire à explorer (répétable).
        #[arg(long = "proton-root")]
        proton_roots: Vec<PathBuf>,
    },
    /// Inspecte un préfixe Wine ou un répertoire `compatdata/<appid>` Proton.
    Prefix { path: PathBuf },
    /// Analyse un journal Wine/Proton/DXVK (`-` pour l'entrée standard).
    AnalyzeLog { path: PathBuf },
    /// Diagnostic complet : environnement + runtimes.
    Doctor {
        #[arg(long = "proton-root")]
        proton_roots: Vec<PathBuf>,
    },
}

#[derive(Serialize)]
struct Report<T: Serialize> {
    #[serde(flatten)]
    data: T,
    findings: Vec<Finding>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(max) => match max {
            Some(Severity::Blocker) => ExitCode::from(3),
            Some(Severity::Error) => ExitCode::from(2),
            _ => ExitCode::SUCCESS,
        },
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(1)
        }
    }
}

fn proton_roots(extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots = extra;
    if let Some(home) = RealSystem.env_var("HOME") {
        roots.extend(redm_runtime::default_proton_roots(&PathBuf::from(home)));
    }
    roots
}

fn run(cli: Cli) -> Result<Option<Severity>> {
    let sys = RealSystem;
    let env = redm_env::collect(&sys);
    let host = env.host;

    match cli.command {
        Command::Env => {
            let findings = redm_env::assess(&env);
            emit(cli.json, "environment", host, &env, findings, |r| {
                print_env(r)
            })
        }
        Command::Runtimes {
            proton_roots: extra,
        } => {
            let protons = redm_runtime::discover_proton(&proton_roots(extra));
            let findings = redm_runtime::assess_runtimes(&protons, env.wine_version.is_some());
            emit(cli.json, "runtimes", host, &protons, findings, |p| {
                if p.is_empty() {
                    println!("Aucune installation Proton trouvée.");
                }
                for i in p {
                    println!(
                        "{}  [{}]  {}",
                        i.name,
                        i.version.as_deref().unwrap_or("?"),
                        i.path.display()
                    );
                }
            })
        }
        Command::Prefix { path } => {
            let r = redm_runtime::inspect_prefix(&path);
            let findings = redm_runtime::assess_prefix(&r);
            emit(cli.json, "prefix", host, &r, findings, |r| {
                println!("Préfixe     : {}", r.path.display());
                println!("Existe      : {}", r.exists);
                println!("Initialisé  : {}", r.initialized);
                println!("Architecture: {:?}", r.arch);
            })
        }
        Command::AnalyzeLog { path } => {
            let analysis = if path.as_os_str() == "-" {
                redm_logs::analyze(io::stdin().lock())?
            } else {
                let f = File::open(&path)
                    .with_context(|| format!("ouverture de {}", path.display()))?;
                redm_logs::analyze(BufReader::new(f))?
            };
            let findings = analysis.findings.clone();
            #[derive(Serialize)]
            struct Meta {
                source: String,
                lines_scanned: usize,
                fixme_lines: usize,
            }
            let meta = Meta {
                source: path.display().to_string(),
                lines_scanned: analysis.lines_scanned,
                fixme_lines: analysis.fixme_lines,
            };
            emit(cli.json, "log-analysis", host, &meta, findings, |m| {
                println!(
                    "{} lignes analysées ({} fixme)",
                    m.lines_scanned, m.fixme_lines
                );
            })
        }
        Command::Doctor {
            proton_roots: extra,
        } => {
            let protons = redm_runtime::discover_proton(&proton_roots(extra));
            let mut findings = redm_env::assess(&env);
            findings.extend(redm_runtime::assess_runtimes(
                &protons,
                env.wine_version.is_some(),
            ));
            #[derive(Serialize)]
            struct Doctor<'a> {
                environment: &'a redm_env::EnvironmentReport,
                proton: &'a [redm_runtime::ProtonInstall],
            }
            let data = Doctor {
                environment: &env,
                proton: &protons,
            };
            emit(cli.json, "doctor", host, &data, findings, |d| {
                print_env(d.environment);
                println!("Proton      : {} installation(s)", d.proton.len());
            })
        }
    }
}

/// Émet un rapport en JSON ou en texte et renvoie la gravité maximale.
fn emit<T: Serialize>(
    json: bool,
    kind: &str,
    host: HostKind,
    data: &T,
    mut findings: Vec<Finding>,
    human: impl FnOnce(&T),
) -> Result<Option<Severity>> {
    redm_core::sort_findings(&mut findings);
    let max = max_severity(&findings);
    let mut out = io::stdout().lock();
    if json {
        let rec = EvidenceRecord::new(kind, host, Report { data, findings });
        serde_json::to_writer_pretty(&mut out, &rec)?;
        writeln!(out)?;
    } else {
        human(data);
        print_findings(&findings);
    }
    Ok(max)
}

fn print_env(r: &redm_env::EnvironmentReport) {
    let opt = |o: Option<&str>| o.unwrap_or("?").to_owned();
    println!("Hôte        : {}", r.host);
    println!("Noyau       : {}", opt(r.kernel_release.as_deref()));
    println!("Système     : {}", opt(r.os.pretty_name.as_deref()));
    println!(
        "CPU         : {} ({} threads)",
        opt(r.cpu_model.as_deref()),
        r.logical_cpus.map_or("?".into(), |n| n.to_string())
    );
    println!(
        "Mémoire     : {}",
        r.memory_total_kib.map_or("?".into(), |k| format!(
            "{:.1} Gio",
            k as f64 / 1024.0 / 1024.0
        ))
    );
    println!(
        "/dev/dxg    : {}",
        if r.dxg_present { "présent" } else { "absent" }
    );
    println!(
        "/dev/dri    : {} · ICD Dozen : {}",
        if r.drm_present { "présent" } else { "absent" },
        if r.dozen_icd_present {
            "installé"
        } else {
            "absent"
        }
    );
    if r.vulkan_devices.is_empty() {
        println!("Vulkan      : aucun périphérique ({:?})", r.vulkan_probe);
    }
    for d in &r.vulkan_devices {
        println!(
            "Vulkan      : {} [{} / API {}]",
            opt(d.name.as_deref()),
            opt(d.driver_info.as_deref().or(d.driver_name.as_deref())),
            opt(d.api_version.as_deref())
        );
    }
    println!("Wine        : {}", opt(r.wine_version.as_deref()));
}

fn print_findings(findings: &[Finding]) {
    if findings.is_empty() {
        println!("\nAucun constat.");
        return;
    }
    println!("\nConstats :");
    for f in findings {
        let count = if f.occurrences > 1 {
            format!(" ×{}", f.occurrences)
        } else {
            String::new()
        };
        let detail = f
            .detail
            .as_deref()
            .map(|d| format!(" — {d}"))
            .unwrap_or_default();
        let loc = f
            .location
            .as_deref()
            .map(|l| format!(" ({l})"))
            .unwrap_or_default();
        println!("  [{:<7}] {}{detail}{count}{loc}", f.severity, f.title);
        println!(
            "            règle {} · résolution : {}",
            f.rule_id, f.resolution
        );
        if let Some(a) = &f.advice {
            println!("            → {a}");
        }
    }
}
