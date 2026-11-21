//! `redm-linux` : point d'entrée de la plateforme.
//!
//! Codes de sortie :
//! - 0 : aucun constat de gravité ≥ error
//! - 1 : échec de l'outil lui-même (E/S, arguments…)
//! - 2 : au moins un constat `error`
//! - 3 : au moins un constat `blocker`

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use redm_core::{EvidenceRecord, Finding, HostKind, Resolution, Severity, max_severity};
use redm_env::{RealSystem, SystemSource};
use redm_game::GameScan;
use redm_runtime::graphics::GraphicsStack;
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
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
    /// Ne pas anonymiser la sortie JSON (e-mails, noms d'utilisateur).
    /// À réserver au débogage local : les preuves sont faites pour être publiées.
    #[arg(long, global = true)]
    no_redact: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Décrit l'hôte : WSL2/natif, CPU, mémoire, Vulkan, Wine.
    Env,
    /// Liste les installations Proton et leurs versions de DXVK/VKD3D-Proton.
    Runtimes {
        /// Racine supplémentaire à explorer (répétable).
        #[arg(long = "proton-root")]
        proton_roots: Vec<PathBuf>,
    },
    /// Analyse un préfixe : état, pile graphique, Rockstar Launcher, RDR2, RedM.
    /// Accepte aussi un répertoire `compatdata/<appid>` Proton.
    Prefix { path: PathBuf },
    /// Découvre les préfixes Wine/Proton/Lutris et analyse ceux qui contiennent
    /// le Rockstar Launcher, RDR2 ou RedM.
    Scan {
        /// Préfixe supplémentaire à analyser (répétable).
        #[arg(long = "prefix")]
        prefixes: Vec<PathBuf>,
        /// Afficher aussi les préfixes sans rapport avec RDR2/RedM.
        #[arg(long)]
        all: bool,
    },
    /// Analyse un journal Wine/Proton/DXVK/CitizenFX (`-` pour l'entrée standard).
    AnalyzeLog { path: PathBuf },
    /// Diagnostic complet : environnement, runtimes et préfixes de jeu.
    Doctor {
        #[arg(long = "proton-root")]
        proton_roots: Vec<PathBuf>,
        #[arg(long = "prefix")]
        prefixes: Vec<PathBuf>,
    },
}

#[derive(Serialize)]
struct Report<T: Serialize> {
    #[serde(flatten)]
    data: T,
    findings: Vec<Finding>,
}

/// Analyse complète d'un préfixe.
#[derive(Serialize)]
struct PrefixScan {
    prefix: redm_runtime::PrefixReport,
    graphics: GraphicsStack,
    game: GameScan,
}

impl PrefixScan {
    fn run(path: &Path) -> Self {
        let prefix = redm_runtime::inspect_prefix(path);
        let graphics = redm_runtime::graphics::inspect_prefix_graphics(&prefix.path);
        let game = redm_game::scan_prefix(&prefix.path);
        Self {
            prefix,
            graphics,
            game,
        }
    }

    fn findings(&self) -> Vec<Finding> {
        let mut f = redm_runtime::assess_prefix(&self.prefix);
        if self.prefix.initialized {
            let loc = self.prefix.path.display().to_string();
            f.extend(
                redm_runtime::graphics::assess_graphics(&self.graphics)
                    .into_iter()
                    .map(|x| x.with_location(loc.clone())),
            );
            f.extend(redm_game::assess_game(&self.game, &self.graphics));
        }
        f
    }
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

fn home() -> Option<PathBuf> {
    RealSystem.env_var("HOME").map(PathBuf::from)
}

fn proton_roots(extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots = extra;
    if let Some(h) = home() {
        roots.extend(redm_runtime::default_proton_roots(&h));
    }
    roots
}

/// Préfixes initialisés : ceux passés explicitement, puis ceux découverts.
fn discover(extra: Vec<PathBuf>) -> Vec<PrefixScan> {
    let mut candidates = extra;
    if let Some(h) = home() {
        candidates.extend(redm_runtime::default_prefix_candidates(&h));
    }
    redm_runtime::discover_prefixes(&candidates)
        .iter()
        .map(|p| PrefixScan::run(&p.path))
        .collect()
}

struct Out {
    json: bool,
    redact: bool,
    host: HostKind,
}

fn run(cli: Cli) -> Result<Option<Severity>> {
    let env = redm_env::collect(&RealSystem);
    let out = Out {
        json: cli.json,
        redact: !cli.no_redact,
        host: env.host,
    };

    match cli.command {
        Command::Env => {
            let findings = redm_env::assess(&env);
            out.emit("environment", &env, findings, print_env)
        }
        Command::Runtimes {
            proton_roots: extra,
        } => {
            let protons = redm_runtime::discover_proton(&proton_roots(extra));
            let findings = redm_runtime::assess_runtimes(&protons, env.wine_version.is_some());
            #[derive(Serialize)]
            struct Runtimes<'a> {
                proton: &'a [redm_runtime::ProtonInstall],
            }
            let data = Runtimes { proton: &protons };
            out.emit("runtimes", &data, findings, |r| print_protons(r.proton))
        }
        Command::Prefix { path } => {
            let scan = PrefixScan::run(&path);
            let findings = scan.findings();
            out.emit("prefix", &scan, findings, print_scan)
        }
        Command::Scan { prefixes, all } => {
            let scans: Vec<_> = discover(prefixes)
                .into_iter()
                .filter(|s| all || s.game.is_relevant())
                .collect();
            let findings = scans.iter().flat_map(PrefixScan::findings).collect();
            #[derive(Serialize)]
            struct Scan<'a> {
                prefixes: &'a [PrefixScan],
            }
            let data = Scan { prefixes: &scans };
            out.emit("prefix-scan", &data, findings, |d| {
                let s = d.prefixes;
                if s.is_empty() {
                    println!("Aucun préfixe contenant le Rockstar Launcher, RDR2 ou RedM.");
                }
                for scan in s {
                    print_scan(scan);
                    println!();
                }
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
            #[derive(Serialize)]
            struct Meta {
                source: String,
                lines_scanned: usize,
                fixme_lines: usize,
                citizenfx: redm_logs::CfxFacts,
            }
            let meta = Meta {
                source: path.display().to_string(),
                lines_scanned: analysis.lines_scanned,
                fixme_lines: analysis.fixme_lines,
                citizenfx: analysis.citizenfx,
            };
            out.emit("log-analysis", &meta, analysis.findings, |m| {
                println!(
                    "{} lignes analysées ({} fixme)",
                    m.lines_scanned, m.fixme_lines
                );
                let c = &m.citizenfx;
                if c.is_citizenfx_log {
                    println!("Journal CitizenFX");
                    if let Some(b) = c.game_build {
                        println!("  Build du jeu   : {b}");
                    }
                    if let Some(v) = &c.game_version {
                        println!("  Version RDR2   : {v}");
                    }
                    if let Some(l) = &c.game_install_location {
                        println!("  Emplacement    : {l}");
                    }
                }
            })
        }
        Command::Doctor {
            proton_roots: extra,
            prefixes,
        } => {
            let protons = redm_runtime::discover_proton(&proton_roots(extra));
            let games: Vec<_> = discover(prefixes)
                .into_iter()
                .filter(|s| s.game.is_relevant())
                .collect();
            let mut findings = redm_env::assess(&env);
            findings.extend(redm_runtime::assess_runtimes(
                &protons,
                env.wine_version.is_some(),
            ));
            findings.extend(games.iter().flat_map(PrefixScan::findings));
            if !games.iter().any(|g| g.game.rdr2.is_some()) {
                findings.push(
                    Finding::new(
                        "game.rdr2-not-found",
                        Severity::Info,
                        "Aucun préfixe contenant RDR2 n'a été trouvé",
                        Resolution::LocalConfiguration,
                    )
                    .with_advice(
                        "Installer le Rockstar Games Launcher dans un préfixe Proton ou Lutris, \
                         ou indiquer le préfixe avec `--prefix <chemin>`.",
                    ),
                );
            }
            #[derive(Serialize)]
            struct Doctor<'a> {
                environment: &'a redm_env::EnvironmentReport,
                proton: &'a [redm_runtime::ProtonInstall],
                game_prefixes: &'a [PrefixScan],
            }
            let data = Doctor {
                environment: &env,
                proton: &protons,
                game_prefixes: &games,
            };
            out.emit("doctor", &data, findings, |d| {
                print_env(d.environment);
                println!();
                print_protons(d.proton);
                for g in d.game_prefixes {
                    println!();
                    print_scan(g);
                }
            })
        }
    }
}

impl Out {
    /// Émet un rapport en JSON ou en texte et renvoie la gravité maximale.
    fn emit<T: Serialize>(
        &self,
        kind: &str,
        data: &T,
        mut findings: Vec<Finding>,
        human: impl FnOnce(&T),
    ) -> Result<Option<Severity>> {
        redm_core::sort_findings(&mut findings);
        let max = max_severity(&findings);
        let mut stdout = io::stdout().lock();
        if self.json {
            let rec = EvidenceRecord::new(kind, self.host, Report { data, findings });
            let text = serde_json::to_string_pretty(&rec)?;
            let text = if self.redact {
                let user = RealSystem.env_var("USER").unwrap_or_default();
                redm_core::redact_with(&text, &[&user])
            } else {
                text
            };
            writeln!(stdout, "{text}")?;
        } else {
            human(data);
            print_findings(&findings);
        }
        Ok(max)
    }
}

fn print_protons(p: &[redm_runtime::ProtonInstall]) {
    if p.is_empty() {
        println!("Proton      : aucune installation trouvée");
    }
    for i in p {
        println!(
            "Proton      : {} [{}] · DXVK {} · VKD3D-Proton {}",
            i.name,
            i.version.as_deref().unwrap_or("?"),
            i.bundled.dxvk_version.as_deref().unwrap_or("?"),
            i.bundled.vkd3d_proton_version.as_deref().unwrap_or("?"),
        );
    }
}

fn print_scan(s: &PrefixScan) {
    let yes_no = |b: bool| if b { "oui" } else { "non" };
    println!("Préfixe     : {}", s.prefix.path.display());
    println!(
        "  État      : initialisé {} · architecture {:?}",
        yes_no(s.prefix.initialized),
        s.prefix.arch
    );
    for d in &s.graphics.dlls {
        println!(
            "  {:<13} : {:?}{}",
            d.name,
            d.kind,
            d.version
                .as_deref()
                .map(|v| format!(" {v}"))
                .unwrap_or_default()
        );
    }
    for (dll, mode) in &s.graphics.dll_overrides {
        println!("  surcharge : {dll} = {mode}");
    }
    let g = &s.game;
    println!(
        "  Launcher  : {}",
        g.rockstar_launcher
            .as_ref()
            .map_or("absent".into(), |p| p.display().to_string())
    );
    match &g.rdr2 {
        Some(r) => println!(
            "  RDR2      : {} ({:?}, API {:?})",
            r.path.display(),
            r.source,
            r.graphics_api
        ),
        None => println!("  RDR2      : absent"),
    }
    match &g.redm {
        Some(m) => println!(
            "  RedM      : {} (exe {}, {} journal(aux), {} crash dump(s))",
            m.dir.display(),
            yes_no(m.exe_present),
            m.latest_logs.len(),
            m.crash_dumps
        ),
        None => println!("  RedM      : absent"),
    }
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
