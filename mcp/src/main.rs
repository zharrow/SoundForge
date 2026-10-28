//! Serveur MCP : expose sound-forge à Claude Code.
//!
//! Lancement : `sound-forge-mcp` (transport stdio).
//! Dossier de travail : variable d'environnement SOUND_OUTPUT_DIR (défaut ./sound_output).
//!
//! Deux familles d'outils : ceux qui rendent des fichiers directement (render_sound…), et ceux qui
//! pilotent l'app Sound Forge ouverte sur le Mac (open_app, edit_sound…), où l'utilisateur entend et
//! voit chaque retouche, et peut l'annuler (⌘Z). Après un rendu, l'app montre le son si elle est ouverte.

mod app;

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use rmcp::{
    ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router, transport::stdio,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sound_forge::recipe::GENERATOR;
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
struct RecipeName {
    /// Nom du son : le nom de base, ou celui d'une variante (nom_02), avec ou sans extension.
    filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RenderRecipe {
    /// Nom du son dont on rejoue la recette (nom de base ou d'une variante).
    filename: String,
    /// Nouveau nom de sortie. Par défaut : le même nom, donc les WAV sont réécrits.
    new_filename: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompareFiles {
    /// Premier WAV, par exemple un SFX.
    first: String,
    /// Second WAV, par exemple une piste de musique.
    second: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RenderMusic {
    /// La partition : tempo, mesures, pistes (instrument du catalogue ou son de synthèse), notes et motifs.
    music: sound_forge::music::Music,
    /// Nom de base des fichiers, dans musique/ : <nom>_<stem>.wav, <nom>_mix.wav, <nom>.json.
    filename: String,
    /// Graine du jeu humain (même graine = mêmes fichiers).
    seed: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MusicName {
    /// Nom d'un morceau de musique/ (sans extension).
    filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct OpenSound {
    /// Son du dossier de travail à ouvrir (nom de base ou d'une variante, avec ou sans extension).
    filename: Option<String>,
    /// Ou bien un preset (voir list_presets), ouvert comme nouveau son.
    preset: Option<String>,
    /// Ouvrir même si le son affiché a des modifications non enregistrées (elles seraient perdues).
    /// Seulement avec l'accord de l'utilisateur.
    force: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NewSound {
    /// Son de départ (même format que render_sound). Sans lui : une couche sinus simple.
    sound: Option<Sound>,
    /// Nombre de variantes (1 à 16).
    variants: Option<u32>,
    /// Écart entre variantes, de 0 à 1.
    amount: Option<f32>,
    /// Graine aléatoire.
    seed: Option<u64>,
    /// Remplacer le son affiché même s'il a des modifications non enregistrées.
    force: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EditSound {
    /// Nouvelle description complète du son ouvert (même format que render_sound). Partir de
    /// get_document, modifier, renvoyer le tout.
    sound: Option<Sound>,
    /// Nombre de variantes (1 à 16).
    variants: Option<u32>,
    /// Écart entre variantes, de 0 à 1.
    amount: Option<f32>,
    /// Graine aléatoire.
    seed: Option<u64>,
    /// Nom court de la retouche dans l'historique de l'app (« attaque plus sèche »…).
    label: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PlaySound {
    /// Variante à écouter, à partir de 1. Par défaut : celle affichée.
    variant: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SaveSound {
    /// Nom de base des fichiers. Par défaut : le nom du son ouvert.
    filename: Option<String>,
    /// Remplacer un autre son du même nom.
    overwrite: Option<bool>,
}

/// Résultat d'un appel à l'app : un résumé lisible, et l'image de contrôle quand l'app la fournit.
fn app_result(result: Result<Value, String>, title: &str) -> ToolResult {
    let mut value = match result {
        Ok(value) => value,
        Err(message) => return Ok(CallToolResult::error(vec![ContentBlock::text(message)])),
    };
    let image = value
        .as_object_mut()
        .and_then(|o| o.remove("overview_png"))
        .and_then(|v| v.as_str().map(str::to_owned));
    let mut blocks = vec![ContentBlock::text(format!("{title}\n{}", describe(&value)))];
    if let Some(png) = image {
        blocks.push(ContentBlock::image(png, "image/png"));
    }
    Ok(CallToolResult::success(blocks))
}

/// Résumé lisible d'une réponse de l'app ; la prise complète (le JSON du son) n'est donnée que si elle est là.
fn describe(value: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(name) = value.get("name") {
        let dirty = value["dirty"].as_bool() == Some(true);
        let state = match (name.as_str(), dirty) {
            (None, false) => "nouveau, pas encore enregistré",
            (None, true) => "nouveau, retouché, pas encore enregistré",
            (Some(_), true) => "modifié, non enregistré",
            (Some(_), false) => "enregistré",
        };
        let name = name.as_str().unwrap_or("sans nom");
        lines.push(format!("Son ouvert : {name} ({state}), variante {}", value["variant"]));
    }
    if let Some(file) = value["file"].as_str() {
        lines.push(format!(
            "{file} : {} s, crête {} dB, RMS {} dB, bandes {}",
            value["duration_s"], value["peak_db"], value["rms_db"], value["bands"]
        ));
    }
    if let Some(files) = value["files"].as_array() {
        lines.push(format!("Fichiers écrits : {}", files.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
    }
    for key in ["muted_layers", "solo_layers"] {
        if value[key].as_array().is_some_and(|a| !a.is_empty()) {
            lines.push(format!("{key} : {} (aide d'écoute de l'utilisateur)", value[key]));
        }
    }
    if let Some(history) = value["history"].as_array() {
        lines.push(format!("Historique récent : {}", history.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ; ")));
    }
    if let Some(take) = value.get("take") {
        lines.push(format!("Prise (son + variantes) :\n{}", serde_json::to_string_pretty(take).unwrap_or_default()));
    }
    if value["playing"].as_bool() == Some(true) {
        lines.push(format!("Lecture : variante {}, {} s", value["variant"], value["duration_s"]));
    }
    for key in ["changed", "opened", "reason"] {
        if let Some(v) = value.get(key) {
            lines.push(format!("{key} : {v}"));
        }
    }
    lines.join("\n")
}

/// Après un rendu : si l'app est ouverte, elle montre le son (sans jamais jeter un travail non enregistré).
fn show_in_app(text: String, filename: &str) -> String {
    match app::call_if_running("show", json!({ "filename": filename })) {
        Some(Ok(reply)) if reply["opened"].as_bool() == Some(true) => format!("{text}\nAffiché dans Sound Forge."),
        Some(Ok(reply)) => format!("{text}\nSound Forge : {}", reply["reason"].as_str().unwrap_or("non affiché")),
        _ => text,
    }
}

fn export_options(variants: Option<u32>, amount: Option<f32>, seed: Option<u64>) -> ExportOptions {
    let defaults = ExportOptions::default();
    ExportOptions {
        variants: variants.unwrap_or(defaults.variants),
        amount: amount.unwrap_or(defaults.amount),
        seed: seed.unwrap_or(defaults.seed),
    }
}

/// `nom_01` -> `nom` quand la recette a produit des variantes numérotées.
fn strip_variant_suffix(file: &str, variants: u32) -> &str {
    if variants <= 1 {
        return file;
    }
    file.rsplit_once('_').map_or(file, |(base, _)| base)
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
                self.workspace
                    .export_with_recipe(&sound, filename, &options, Some(&params.name))
            })
            .map(|reports| {
                show_in_app(format_reports(&reports), params.filename.as_deref().unwrap_or(&params.name))
            });
        text_result(result)
    }

    #[tool(description = "Rend un son décrit en JSON (couches + effets) en WAV. \
        Un son se construit en couches : attaque courte (bruit filtré), corps (oscillateur), \
        queue (réverbération). Une couche peut jouer un enregistrement de l'utilisateur : \
        waveform {\"sample\": {\"file\": \"voix/nom\", \"reverse\": false}}, la hauteur réglant la vitesse \
        (440 Hz = vitesse d'origine, 880 = une octave plus haut) ; régler la tenue de l'enveloppe pour \
        l'entendre en entier. Vérifier ensuite avec spectrogram.")]
    fn render_sound(&self, Parameters(params): Parameters<RenderSound>) -> ToolResult {
        let options = export_options(params.variants, params.amount, params.seed);
        let result = params
            .sound
            .check()
            .and_then(|()| {
                self.workspace
                    .export_with_recipe(&params.sound, &params.filename, &options, None)
            })
            .map(|reports| show_in_app(format_reports(&reports), &params.filename));
        text_result(result)
    }

    #[tool(
        description = "Renvoie la recette JSON d'un son déjà rendu : le son, la graine et \
        les réglages de variantes. Point de départ pour retoucher un son existant : modifier \
        `sound` puis le rendre avec render_sound."
    )]
    fn get_recipe(&self, Parameters(params): Parameters<RecipeName>) -> ToolResult {
        text_result(
            self.workspace
                .read_recipe(&params.filename)
                .and_then(|recipe| recipe.to_json()),
        )
    }

    #[tool(
        description = "Rejoue la recette d'un son à l'identique (même graine, mêmes \
        variantes). Sert à régénérer un son après une mise à jour du moteur, ou à le dupliquer \
        sous un autre nom."
    )]
    fn render_recipe(&self, Parameters(params): Parameters<RenderRecipe>) -> ToolResult {
        let result = self
            .workspace
            .read_recipe(&params.filename)
            .and_then(|recipe| {
                let base = recipe
                    .files
                    .first()
                    .and_then(|f| f.strip_suffix(".wav"))
                    .map(|f| strip_variant_suffix(f, recipe.variants))
                    .unwrap_or(&params.filename)
                    .to_owned();
                let target = params.new_filename.unwrap_or(base);
                let note = if recipe.generator == GENERATOR {
                    String::new()
                } else {
                    format!(
                        "\nRecette écrite par {}, rejouée par {GENERATOR} : le son peut différer.",
                        recipe.generator
                    )
                };
                recipe.sound.check()?;
                self.workspace
                    .export_with_recipe(
                        &recipe.sound,
                        &target,
                        &recipe.options(),
                        recipe.preset.as_deref(),
                    )
                    .map(|reports| show_in_app(format_reports(&reports) + &note, &target))
            });
        text_result(result)
    }

    #[tool(description = "Liste les WAV, les recettes (.json) et les enregistrements de l'utilisateur \
        (voix/…) du dossier de travail.")]
    fn list_files(&self) -> ToolResult {
        let files = self.workspace.list_wavs();
        let recipes = self.workspace.list_recipes();
        let recordings = self.workspace.list_recordings();
        let music = self.workspace.list_music();
        let root = self.workspace.root().display();
        text_result(Ok(format!(
            "Dossier : {root}\nWAV :\n{}\nRecettes :\n{}\nEnregistrements (source `sample`) :\n{}\nMorceaux (musique/) :\n{}",
            files.join("\n"),
            recipes.join("\n"),
            recordings.join("\n"),
            music.join("\n")
        )))
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

    // ---------- Musique : partitions jouées par de vrais instruments ----------

    #[tool(description = "(Re)construit le catalogue des instruments échantillonnés à partir de \
        selection.json du dossier des instruments : lit note, nuance et prise dans le nom des fichiers \
        et corrige l'octave d'après la hauteur mesurée. À relancer après avoir modifié la sélection.")]
    fn build_instruments(&self) -> ToolResult {
        text_result(self.workspace.build_instruments())
    }

    #[tool(description = "Liste les instruments jouables par render_music : identifiant, nom, famille, \
        jeu (tenu : la note dure ce qu'on demande ; frappe : l'échantillon joue en entier), tessiture \
        (notes MIDI et noms, la3 = A4 = 440 Hz) et couches de nuance.")]
    fn list_instruments(&self) -> ToolResult {
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let name = |n: u8| format!("{}{}", names[usize::from(n % 12)], i32::from(n / 12) - 1);
        text_result(self.workspace.instruments().map(|catalogue| {
            catalogue
                .instruments
                .iter()
                .map(|i| {
                    let kit = if i.zones.iter().all(|z| z.note == i.zones[0].note) || i.family == "percussions" {
                        let mut notes: Vec<u8> = i.zones.iter().map(|z| z.note).collect();
                        notes.dedup();
                        format!(" ; kit : {}", notes.iter().map(|n| name(*n)).collect::<Vec<_>>().join(" "))
                    } else {
                        String::new()
                    };
                    format!(
                        "{:<22} {} ({}) · {:?} · {}–{} · {} nuance(s){kit}",
                        i.id, i.name, i.family, i.kind, name(i.range.0), name(i.range.1), i.layers
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }))
    }

    #[tool(description = "Rend un morceau de musique en stems stéréo synchronisés, de longueur exacte \
        (bars × temps), dans musique/ : un WAV par stem, le mélange (<nom>_mix) et la recette. En boucle \
        (looping), la queue revient sur le début : couture sans trou. Un seul gain pour tous les stems \
        (jamais normalisés séparément). Notes : at et len en temps, note « D4 » (A4 = 440 Hz) ou MIDI, vel \
        0..1 ; motifs (patterns) réutilisables via play. instrument : un identifiant de list_instruments, ou \
        {\"synth\": son} (format render_sound) pour nappes et textures. Renvoie les mesures et l'image du \
        mélange.")]
    fn render_music(&self, Parameters(params): Parameters<RenderMusic>) -> ToolResult {
        let seed = params.seed.unwrap_or(1);
        match self.workspace.render_music(&params.music, &params.filename, seed) {
            Ok(reports) => {
                let mix = reports.last().map(|r| r.file.clone());
                let text = format!(
                    "{}\nBoucle : {:.2} s, {} mesures à {} BPM.",
                    format_reports(&reports),
                    params.music.duration(),
                    params.music.bars,
                    params.music.bpm
                );
                let mut blocks = vec![ContentBlock::text(show_in_app(text, &format!("musique/{}", params.filename)))];
                if let Some(Ok(png)) = mix.map(|p| sound_forge::report::spectrogram_file(&p)) {
                    blocks.push(ContentBlock::image(base64::encode(&png), "image/png"));
                }
                Ok(CallToolResult::success(blocks))
            }
            Err(message) => Ok(CallToolResult::error(vec![ContentBlock::text(message)])),
        }
    }

    #[tool(description = "Renvoie la recette d'un morceau de musique/ (partition complète, graine, \
        fichiers) : point de départ pour le retoucher puis le rendre à nouveau avec render_music.")]
    fn get_music(&self, Parameters(params): Parameters<MusicName>) -> ToolResult {
        text_result(
            self.workspace
                .read_music(&params.filename)
                .and_then(|r| serde_json::to_string_pretty(&r).map_err(|e| e.to_string())),
        )
    }

    // ---------- L'app Sound Forge : retouches en direct, sous les yeux de l'utilisateur ----------

    #[tool(description = "Ouvre l'app Sound Forge (éditeur visuel de sons) au premier plan et décrit le \
        son affiché. L'utilisateur y voit et entend chaque retouche faite par les outils open_sound, \
        edit_sound, play_sound, save_sound, et peut les annuler (⌘Z).")]
    fn open_app(&self) -> ToolResult {
        let result = app::bring_to_front().and_then(|()| app::call("get_document", json!({})));
        app_result(result, "Sound Forge est ouvert.")
    }

    #[tool(description = "Décrit le son ouvert dans l'app Sound Forge : nom, état (enregistré ou non), \
        variante écoutée, historique récent (retouches de l'utilisateur et de Claude) et la prise complète \
        en JSON (son + variantes), à modifier puis renvoyer avec edit_sound. Lance l'app si besoin.")]
    fn get_document(&self) -> ToolResult {
        app_result(app::call("get_document", json!({})), "Son ouvert dans Sound Forge :")
    }

    #[tool(description = "Ouvre dans l'app Sound Forge un son du dossier de travail (filename) ou un \
        preset (preset), le fait entendre et renvoie ses mesures et son image de contrôle. Refusé si le son \
        affiché a des modifications non enregistrées (force=true seulement avec l'accord de l'utilisateur).")]
    fn open_sound(&self, Parameters(params): Parameters<OpenSound>) -> ToolResult {
        let result = app::call(
            "open_sound",
            json!({ "filename": params.filename, "preset": params.preset, "force": params.force }),
        );
        app_result(result, "Ouvert dans Sound Forge.")
    }

    #[tool(description = "Crée un nouveau son dans l'app Sound Forge, vide ou à partir d'un JSON de son. \
        Rien n'est écrit sur le disque avant save_sound.")]
    fn new_sound(&self, Parameters(params): Parameters<NewSound>) -> ToolResult {
        let result = app::call(
            "new_sound",
            json!({ "sound": params.sound, "variants": params.variants, "amount": params.amount,
                    "seed": params.seed, "force": params.force }),
        );
        app_result(result, "Nouveau son dans Sound Forge.")
    }

    #[tool(description = "Retouche le son ouvert dans l'app Sound Forge : remplace sa description \
        (sound, au format de render_sound) et/ou ses réglages de variantes. La retouche entre dans \
        l'historique de l'app à ton nom (l'utilisateur peut l'annuler avec ⌘Z) et s'entend aussitôt. \
        Renvoie les mesures et l'image de contrôle de la variante écoutée. Rien n'est écrit sur le disque \
        avant save_sound.")]
    fn edit_sound(&self, Parameters(params): Parameters<EditSound>) -> ToolResult {
        let result = app::call(
            "edit_sound",
            json!({ "sound": params.sound, "variants": params.variants, "amount": params.amount,
                    "seed": params.seed, "label": params.label }),
        );
        app_result(result, "Retouche appliquée dans Sound Forge.")
    }

    #[tool(description = "Fait entendre à l'utilisateur le son ouvert dans l'app Sound Forge \
        (éventuellement une variante précise, à partir de 1).")]
    fn play_sound(&self, Parameters(params): Parameters<PlaySound>) -> ToolResult {
        app_result(app::call("play", json!({ "variant": params.variant })), "Lecture dans Sound Forge.")
    }

    #[tool(description = "Enregistre le son ouvert dans l'app Sound Forge : WAV (et variantes) et \
        recette JSON dans le dossier de travail, comme render_sound. Sans filename : sous son nom actuel.")]
    fn save_sound(&self, Parameters(params): Parameters<SaveSound>) -> ToolResult {
        let result = app::call(
            "save_sound",
            json!({ "filename": params.filename, "overwrite": params.overwrite }),
        );
        app_result(result, "Enregistré depuis Sound Forge.")
    }
}

#[tool_handler(
    name = "sound-forge",
    version = "0.6.0",
    instructions = "Création d'effets sonores pour jeu vidéo (WAV mono 44,1 kHz pour Godot). \
        Workflow : list_presets → get_preset pour partir d'un son existant → modifier le JSON → \
        render_sound → spectrogram pour vérifier visuellement → corriger. Pour les sons joués \
        souvent (pas, coups), générer 3 à 5 variantes. compare_sounds vérifie qu'un SFX ne se \
        fait pas masquer par la musique. Chaque rendu écrit sa recette (nom.json) à côté des WAV : \
        get_recipe pour retoucher un son existant, render_recipe pour le refaire à l'identique. \
        Les fichiers restent dans le dossier de travail. L'app Sound Forge (open_app) est l'éditeur \
        visuel : pour retoucher un son avec l'utilisateur, l'ouvrir (open_sound), le modifier \
        (get_document → edit_sound) : il entend chaque retouche et peut l'annuler ; puis save_sound. \
        Après un render_*, l'app affiche le son si elle est ouverte. L'utilisateur peut enregistrer sa \
        voix dans l'app (voix/nom.wav) : list_files les liste, analyze_sound et spectrogram les \
        écoutent (filename « voix/nom »), et une couche `sample` s'en sert comme source."
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
