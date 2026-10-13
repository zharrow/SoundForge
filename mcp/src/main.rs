//! Serveur MCP : expose sound-forge à Claude Code.
//!
//! Lancement : `sound-forge-mcp` (transport stdio).
//! Dossier de travail : variable d'environnement SOUND_OUTPUT_DIR (défaut ./sound_output).

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use rmcp::{
    ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router, transport::stdio,
};
use serde::Deserialize;
use sound_forge::{ExportOptions, Report, Sound, Workspace, base64, presets};

type ToolResult = Result<CallToolResult, ErrorData>;

#[derive(Clone)]
struct SoundForge {
    workspace: Workspace,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PresetName {
    /// Nom du preset, voir list_presets.
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RenderPreset {
    /// Nom du preset, voir list_presets.
    name: String,
    /// Nom du fichier sans extension (lettres, chiffres, _ et -). Par défaut : le nom du preset.
    filename: Option<String>,
    /// Nombre de variantes (1 à 16). Plus de 1 produit nom_01.wav, nom_02.wav…
    variants: Option<u32>,
    /// Écart entre variantes, de 0 à 1 (0.4 par défaut).
    amount: Option<f32>,
    /// Graine aléatoire : même graine = même son.
    seed: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RenderSound {
    /// Description complète du son (même format que get_preset).
    sound: Sound,
    /// Nom du fichier sans extension (lettres, chiffres, _ et -).
    filename: String,
    /// Nombre de variantes (1 à 16).
    variants: Option<u32>,
    /// Écart entre variantes, de 0 à 1.
    amount: Option<f32>,
    /// Graine aléatoire.
    seed: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FileName {
    /// Nom d'un WAV du dossier de travail, avec ou sans « .wav ».
    filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompareFiles {
    /// Premier WAV, par exemple un SFX.
    first: String,
    /// Second WAV, par exemple une piste de musique.
    second: String,
}

fn export_options(variants: Option<u32>, amount: Option<f32>, seed: Option<u64>) -> ExportOptions {
    let defaults = ExportOptions::default();
    ExportOptions {
        variants: variants.unwrap_or(defaults.variants),
        amount: amount.unwrap_or(defaults.amount),
        seed: seed.unwrap_or(defaults.seed),
    }
}

/// Erreurs d'outil visibles par le modèle, qui peut corriger son appel.
fn text_result(result: Result<String, String>) -> ToolResult {
    Ok(match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
    })
}

fn format_reports(reports: &[Report]) -> String {
    let lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
    format!(
        "{} fichier(s) écrit(s) :\n{}",
        reports.len(),
        lines.join("\n")
    )
}

#[tool_router]
impl SoundForge {
    #[tool(
        description = "Liste les presets d'effets sonores disponibles, avec une description de chacun."
    )]
    fn list_presets(&self) -> ToolResult {
        let lines: Vec<String> = presets::CATALOG
            .iter()
            .map(|p| format!("{:<16} {}", p.name, p.description))
            .collect();
        text_result(Ok(lines.join("\n")))
    }

    #[tool(description = "Renvoie la description JSON complète d'un preset. \
        Point de départ idéal : modifier ce JSON puis le rendre avec render_sound.")]
    fn get_preset(&self, Parameters(params): Parameters<PresetName>) -> ToolResult {
        let result = presets::by_name(&params.name)
            .ok_or_else(|| format!("Preset inconnu : {}", params.name))
            .and_then(|sound| serde_json::to_string_pretty(&sound).map_err(|e| e.to_string()));
        text_result(result)
    }

    #[tool(
        description = "Rend un preset en WAV mono 44,1 kHz dans le dossier de travail, \
        éventuellement en plusieurs variantes. Renvoie durée, crête, RMS et bande dominante."
    )]
    fn render_preset(&self, Parameters(params): Parameters<RenderPreset>) -> ToolResult {
        let result = presets::by_name(&params.name)
            .ok_or_else(|| format!("Preset inconnu : {}", params.name))
            .and_then(|sound| {
                let filename = params.filename.as_deref().unwrap_or(&params.name);
                let options = export_options(params.variants, params.amount, params.seed);
                self.workspace.export(&sound, filename, &options)
            })
            .map(|reports| format_reports(&reports));
        text_result(result)
    }

    #[tool(description = "Rend un son décrit en JSON (couches + effets) en WAV. \
        Un son se construit en couches : attaque courte (bruit filtré), corps (oscillateur), \
        queue (réverbération). Vérifier ensuite avec spectrogram.")]
    fn render_sound(&self, Parameters(params): Parameters<RenderSound>) -> ToolResult {
        let options = export_options(params.variants, params.amount, params.seed);
        let result = params
            .sound
            .check()
            .and_then(|()| {
                self.workspace
                    .export(&params.sound, &params.filename, &options)
            })
            .map(|reports| format_reports(&reports));
        text_result(result)
    }

    #[tool(description = "Liste les WAV présents dans le dossier de travail.")]
    fn list_files(&self) -> ToolResult {
        let files = self.workspace.list_wavs();
        let root = self.workspace.root().display();
        text_result(Ok(format!("Dossier : {root}\n{}", files.join("\n"))))
    }

    #[tool(
        description = "Analyse un WAV : durée, crête, RMS et répartition de l'énergie \
        sur 7 bandes (sub, basses, bas-médiums, médiums, haut-médiums, présence, air)."
    )]
    fn analyze_sound(&self, Parameters(params): Parameters<FileName>) -> ToolResult {
        text_result(
            self.workspace
                .analyze(&params.filename)
                .map(|r| r.detailed()),
        )
    }

    #[tool(
        description = "Image d'un WAV pour le « voir » : forme d'onde en haut, spectrogramme \
        en dessous. Temps de gauche à droite. Axe vertical logarithmique de 40 Hz (bas) à 20 kHz \
        (haut), lignes pointillées à 100 Hz, 1 kHz et 10 kHz. Noir = silence, violet puis orange \
        puis jaune pâle = de plus en plus fort."
    )]
    fn spectrogram(&self, Parameters(params): Parameters<FileName>) -> ToolResult {
        let report = self.workspace.analyze(&params.filename);
        let image = self.workspace.spectrogram_png(&params.filename);
        Ok(match (report, image) {
            (Ok(report), Ok(png)) => CallToolResult::success(vec![
                ContentBlock::text(report.to_string()),
                ContentBlock::image(base64::encode(&png), "image/png"),
            ]),
            (Err(message), _) | (_, Err(message)) => {
                CallToolResult::error(vec![ContentBlock::text(message)])
            }
        })
    }

    #[tool(
        description = "Compare deux WAV par bandes de fréquences pour détecter le masquage \
        (un son qui en couvre un autre au mixage), par exemple un SFX contre la musique."
    )]
    fn compare_sounds(&self, Parameters(params): Parameters<CompareFiles>) -> ToolResult {
        let result = self
            .workspace
            .compare(&params.first, &params.second)
            .map(|(a, b, overlap)| format!("{}\n{}\n{overlap}", a.detailed(), b.detailed()));
        text_result(result)
    }
}

#[tool_handler(
    name = "sound-forge",
    version = "0.2.0",
    instructions = "Création d'effets sonores pour jeu vidéo (WAV mono 44,1 kHz pour Godot). \
        Workflow : list_presets → get_preset pour partir d'un son existant → modifier le JSON → \
        render_sound → spectrogram pour vérifier visuellement → corriger. Pour les sons joués \
        souvent (pas, coups), générer 3 à 5 variantes. compare_sounds vérifie qu'un SFX ne se \
        fait pas masquer par la musique. Les fichiers restent dans le dossier de travail."
)]
impl ServerHandler for SoundForge {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = std::env::var_os("SOUND_OUTPUT_DIR")
        .map_or_else(|| PathBuf::from("sound_output"), PathBuf::from);
    let server = SoundForge {
        workspace: Workspace::new(root),
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
