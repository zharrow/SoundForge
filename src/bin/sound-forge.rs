//! CLI : rend les presets en WAV prêts pour Godot, et analyse des fichiers existants.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sound_forge::report::{analyze_file, compare_files, spectrogram_file};
use sound_forge::{ExportOptions, Workspace, presets};

const USAGE: &str = "\
Usage :
  sound-forge list
  sound-forge render <preset|all> [--out DIR] [--variants N] [--amount 0..1] [--seed N]
  sound-forge analyze <fichier.wav>
  sound-forge compare <a.wav> <b.wav>
  sound-forge spectrogram <fichier.wav> <image.png>

Exemples :
  sound-forge render sword_swing --variants 4 --out assets/sfx
  sound-forge compare assets/sfx/sword_swing.wav music/combat_lead.wav";

struct RenderArgs {
    target: String,
    out: PathBuf,
    options: ExportOptions,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let rest = args.get(1..).unwrap_or_default();
    match (args.first().map(String::as_str), rest) {
        (Some("list"), _) => {
            list();
            Ok(())
        }
        (Some("render"), _) => render_command(&parse_render_args(rest)?),
        (Some("analyze"), [file]) => print_analysis(Path::new(file)),
        (Some("compare"), [a, b]) => print_comparison(Path::new(a), Path::new(b)),
        (Some("spectrogram"), [wav, png]) => write_spectrogram(Path::new(wav), Path::new(png)),
        _ => Err(USAGE.to_owned()),
    }
}

fn list() {
    for preset in &presets::CATALOG {
        println!("{:<16} {}", preset.name, preset.description);
    }
}

fn parse_render_args(args: &[String]) -> Result<RenderArgs, String> {
    let target = args.first().ok_or(USAGE)?.clone();
    let mut parsed = RenderArgs {
        target,
        out: PathBuf::from("sfx"),
        options: ExportOptions::default(),
    };
    let mut flags = args[1..].iter();
    while let Some(flag) = flags.next() {
        let value = flags
            .next()
            .ok_or_else(|| format!("Valeur manquante après {flag}"))?;
        apply_flag(&mut parsed, flag, value)?;
    }
    Ok(parsed)
}

fn apply_flag(parsed: &mut RenderArgs, flag: &str, value: &str) -> Result<(), String> {
    match flag {
        "--out" => parsed.out = PathBuf::from(value),
        "--variants" => parsed.options.variants = parse_value(flag, value)?,
        "--amount" => parsed.options.amount = parse_value(flag, value)?,
        "--seed" => parsed.options.seed = parse_value(flag, value)?,
        _ => return Err(format!("Option inconnue : {flag}\n\n{USAGE}")),
    }
    Ok(())
}

fn parse_value<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("Valeur invalide pour {flag} : {value}"))
}

fn render_command(args: &RenderArgs) -> Result<(), String> {
    let names: Vec<&str> = if args.target == "all" {
        presets::names().collect()
    } else {
        vec![args.target.as_str()]
    };
    let workspace = Workspace::new(&args.out);
    for name in names {
        let sound = presets::by_name(name).ok_or_else(|| format!("Preset inconnu : {name}"))?;
        for report in workspace.export(&sound, name, &args.options)? {
            println!("{report}");
        }
    }
    Ok(())
}

fn print_analysis(path: &Path) -> Result<(), String> {
    println!("{}", analyze_file(path)?.detailed());
    Ok(())
}

fn print_comparison(a: &Path, b: &Path) -> Result<(), String> {
    let (first, second, overlap) = compare_files(a, b)?;
    println!("{}\n{}\n{overlap}", first.detailed(), second.detailed());
    Ok(())
}

fn write_spectrogram(wav: &Path, png: &Path) -> Result<(), String> {
    let image = spectrogram_file(wav)?;
    std::fs::write(png, image).map_err(|e| format!("{} : {e}", png.display()))?;
    println!("Spectrogramme écrit : {}", png.display());
    Ok(())
}
