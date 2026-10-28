//! Barre de menus macOS. Chaque élément envoie son identifiant à la page (événement « menu »).
//!
//! Seuls les raccourcis avec ⌘ sont déclarés ici : un raccourci sans modificateur intercepterait
//! la frappe dans les champs. Espace (écouter) et les chiffres (variantes) sont gérés par la page.

use tauri::menu::{AboutMetadataBuilder, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter, Wry};

type Entry = (&'static str, &'static str, Option<&'static str>);

const FILE: &[Entry] = &[
    ("new", "Nouveau son", Some("CmdOrCtrl+N")),
    ("-", "", None),
    ("save", "Enregistrer", Some("CmdOrCtrl+S")),
    ("save_as", "Enregistrer sous…", Some("Shift+CmdOrCtrl+S")),
    ("revert", "Revenir à la version enregistrée", None),
    ("-", "", None),
    ("reveal", "Afficher dans le Finder", Some("Alt+CmdOrCtrl+R")),
];

const EDIT: &[Entry] = &[
    ("undo", "Annuler", Some("CmdOrCtrl+Z")),
    ("redo", "Rétablir", Some("Shift+CmdOrCtrl+Z")),
    ("-", "", None),
    ("cut", "Couper", Some("CmdOrCtrl+X")),
    ("copy", "Copier", Some("CmdOrCtrl+C")),
    ("paste", "Coller", Some("CmdOrCtrl+V")),
    ("select_all", "Tout sélectionner", Some("CmdOrCtrl+A")),
];

const SOUND: &[Entry] = &[
    ("add_layer", "Nouvelle couche", Some("Shift+CmdOrCtrl+N")),
    ("duplicate_layer", "Dupliquer la couche", Some("CmdOrCtrl+D")),
    ("delete_layer", "Supprimer la couche", None),
    ("-", "", None),
    ("play", "Écouter / arrêter", None),
    ("prev_variant", "Variante précédente", Some("CmdOrCtrl+Left")),
    ("next_variant", "Variante suivante", Some("CmdOrCtrl+Right")),
    ("new_seed", "Nouvelle graine", Some("CmdOrCtrl+G")),
    ("-", "", None),
    ("record", "Enregistrer ma voix…", Some("CmdOrCtrl+R")),
    ("compare", "Comparer avant / après", Some("CmdOrCtrl+B")),
];

const HELP: &[Entry] = &[("shortcuts", "Raccourcis clavier", Some("CmdOrCtrl+/"))];

pub fn install(app: &mut App) -> tauri::Result<()> {
    let about = AboutMetadataBuilder::new()
        .name(Some("Sound Forge"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .comments(Some("Éditeur d'effets sonores pour jeux vidéo, pilotable par Claude Code."))
        .build();
    let app_menu = SubmenuBuilder::new(app, "Sound Forge")
        .about(Some(about))
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let mut menu = MenuBuilder::new(app).item(&app_menu);
    for (title, entries) in [("Fichier", FILE), ("Édition", EDIT), ("Son", SOUND)] {
        menu = menu.item(&submenu(app, title, entries)?);
    }
    let window_menu = SubmenuBuilder::new(app, "Fenêtre")
        .minimize()
        .maximize()
        .separator()
        .fullscreen()
        .build()?;
    menu = menu.item(&window_menu).item(&submenu(app, "Aide", HELP)?);

    app.set_menu(menu.build()?)?;
    app.on_menu_event(|handle, event| {
        let _ = handle.emit_to("main", "menu", event.id().0.as_str());
    });
    Ok(())
}

fn submenu(app: &App, title: &str, entries: &[Entry]) -> tauri::Result<tauri::menu::Submenu<Wry>> {
    let mut builder = SubmenuBuilder::new(app, title);
    for &(id, label, accelerator) in entries {
        if id == "-" {
            builder = builder.separator();
        } else {
            builder = builder.item(&item(app, id, label, accelerator)?);
        }
    }
    builder.build()
}

fn item(app: &App, id: &str, label: &str, accelerator: Option<&str>) -> tauri::Result<MenuItem<Wry>> {
    let mut builder = MenuItemBuilder::with_id(id, label);
    if let Some(accelerator) = accelerator {
        builder = builder.accelerator(accelerator);
    }
    builder.build(app)
}
