//! Presets pour un jeu d'action / plateforme (style Dead Cells, Celeste).
//! Chacun est un point de départ : on l'ajuste, ou on en génère des variantes.

use crate::envelope::{Adsr, Sweep};
use crate::filter::FilterMode::{BandPass, HighPass, LowPass};
use crate::oscillator::Waveform::{BrownNoise, Saw, Sine, Triangle, WhiteNoise};
use crate::resonator::{Material, Resonator};
use crate::sound::{Layer, Sound};

pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    build: fn() -> Sound,
}

pub const CATALOG: [Preset; 20] = [
    preset("jump", "Saut léger : ton qui monte + souffle d'air", jump),
    preset(
        "dash",
        "Dash : whoosh qui balaie vers l'aigu + corps grave",
        dash,
    ),
    preset(
        "sword_swing",
        "Coup d'épée dans le vide : whoosh qui descend",
        sword_swing,
    ),
    preset(
        "impact",
        "Coup qui touche : thump grave saturé + craquement",
        impact,
    ),
    preset(
        "pickup",
        "Ramassage (pièce, cellule) : deux notes brillantes avec écho",
        pickup,
    ),
    preset(
        "powerup",
        "Amélioration : arpège montant en supersaw",
        powerup,
    ),
    preset(
        "explosion",
        "Explosion : grondement, boom et claquement",
        explosion,
    ),
    preset(
        "death",
        "Mort du joueur : chute de hauteur avec vibrato",
        death,
    ),
    preset(
        "ui_click",
        "Clic d'interface très court et discret",
        ui_click,
    ),
    preset(
        "ui_confirm",
        "Validation d'interface : deux notes douces",
        ui_confirm,
    ),
    preset(
        "riser",
        "Transition musicale : montée de tension sur 2 s (1 mesure à 120 BPM)",
        riser,
    ),
    preset(
        "downlifter",
        "Transition musicale : retombée après un temps fort",
        downlifter,
    ),
    preset(
        "stinger_victory",
        "Jingle de victoire : arpège puis accord majeur",
        stinger_victory,
    ),
    preset(
        "stinger_boss",
        "Arrivée du boss : impact grave et accord dissonant",
        stinger_boss,
    ),
    preset(
        "wood_knock",
        "Objet (modal) : toc de bois creux, sec et boisé",
        wood_knock,
    ),
    preset(
        "stone_hit",
        "Objet (modal) : pierre ou basalte frappé, choc mat et dense",
        stone_hit,
    ),
    preset(
        "metal_clang",
        "Objet (modal) : plaque de métal frappée, résonance qui tremble",
        metal_clang,
    ),
    preset(
        "bell",
        "Objet (modal) : cloche, bourdon grave et longue queue",
        bell,
    ),
    preset(
        "glass_tap",
        "Objet (modal) : verre, cristal ou nacre effleuré",
        glass_tap,
    ),
    preset(
        "bone_knock",
        "Objet (modal) : os ou carapace creuse, toc sourd",
        bone_knock,
    ),
];

const fn preset(name: &'static str, description: &'static str, build: fn() -> Sound) -> Preset {
    Preset {
        name,
        description,
        build,
    }
}

pub fn names() -> impl Iterator<Item = &'static str> {
    CATALOG.iter().map(|p| p.name)
}

pub fn by_name(name: &str) -> Option<Sound> {
    CATALOG.iter().find(|p| p.name == name).map(|p| (p.build)())
}

/// Fréquence (Hz) d'une note MIDI : 69 = La 440 Hz, 60 = Do central.
pub fn midi(note: u8) -> f32 {
    440.0 * 2_f32.powf((f32::from(note) - 69.0) / 12.0)
}

/// Les couches de bruit ignorent la hauteur : seule la coupure du filtre compte.
fn noise() -> Sweep {
    Sweep::constant(1.0)
}

fn jump() -> Sound {
    let tone = Layer::new(
        Triangle,
        Sweep::new(260.0, 620.0, 0.12),
        Adsr::percussive(0.004, 0.14),
    )
    .filter(LowPass, Sweep::constant(3000.0), 0.2)
    .gain(0.7);
    let air = Layer::new(WhiteNoise, noise(), Adsr::percussive(0.01, 0.14))
        .filter(BandPass, Sweep::new(1200.0, 4500.0, 0.14), 0.5)
        .gain(0.35);
    Sound::new("jump")
        .layer(tone)
        .layer(air)
        .reverb(0.2, 0.4, 0.12)
        .peak_db(-3.0)
}

fn dash() -> Sound {
    let whoosh = Layer::new(WhiteNoise, noise(), Adsr::new(0.03, 0.18, 0.0, 0.0, 0.0))
        .filter(BandPass, Sweep::new(500.0, 6000.0, 0.2), 0.65)
        .gain(0.9);
    let body = Layer::new(
        Saw,
        Sweep::new(140.0, 55.0, 0.15),
        Adsr::percussive(0.005, 0.15),
    )
    .filter(LowPass, Sweep::constant(500.0), 0.3)
    .gain(0.4);
    Sound::new("dash")
        .layer(whoosh)
        .layer(body)
        .reverb(0.3, 0.5, 0.15)
        .peak_db(-2.0)
}

fn sword_swing() -> Sound {
    let blade = Layer::new(WhiteNoise, noise(), Adsr::new(0.04, 0.1, 0.0, 0.0, 0.0))
        .filter(BandPass, Sweep::new(3500.0, 900.0, 0.14), 0.75)
        .gain(1.0);
    let body = Layer::new(WhiteNoise, noise(), Adsr::new(0.03, 0.12, 0.0, 0.0, 0.0))
        .filter(LowPass, Sweep::new(1500.0, 300.0, 0.14), 0.3)
        .gain(0.35);
    Sound::new("sword_swing")
        .layer(blade)
        .layer(body)
        .reverb(0.2, 0.5, 0.1)
        .peak_db(-3.0)
}

fn impact() -> Sound {
    let thump = Layer::new(
        Sine,
        Sweep::new(170.0, 45.0, 0.1),
        Adsr::percussive(0.001, 0.2),
    )
    .gain(1.0);
    let crunch = Layer::new(WhiteNoise, noise(), Adsr::percussive(0.0, 0.07))
        .filter(LowPass, Sweep::new(5000.0, 500.0, 0.07), 0.3)
        .gain(0.6);
    Sound::new("impact")
        .layer(thump)
        .layer(crunch)
        .drive(2.5)
        .reverb(0.25, 0.5, 0.1)
        .peak_db(-1.0)
}

fn pickup() -> Sound {
    let first = Layer::new(
        Triangle,
        Sweep::constant(midi(83)),
        Adsr::percussive(0.003, 0.12),
    );
    let second = Layer::new(
        Triangle,
        Sweep::constant(midi(88)),
        Adsr::percussive(0.003, 0.3),
    )
    .start_at(0.07);
    let shimmer = Layer::new(
        Sine,
        Sweep::constant(midi(100)),
        Adsr::percussive(0.003, 0.25),
    )
    .start_at(0.07)
    .gain(0.2);
    Sound::new("pickup")
        .layer(first.gain(0.6))
        .layer(second.gain(0.7))
        .layer(shimmer)
        .echo(0.09, 0.3, 0.25)
        .reverb(0.35, 0.3, 0.18)
        .peak_db(-4.0)
}

fn powerup() -> Sound {
    let arpeggio = [72, 76, 79, 84].iter().enumerate().map(|(i, &note)| {
        Layer::new(
            Saw,
            Sweep::constant(midi(note)),
            Adsr::new(0.005, 0.1, 0.4, 0.05, 0.25),
        )
        .filter(LowPass, Sweep::new(1200.0, 4000.0, 0.3), 0.3)
        .unison(3, 14.0)
        .gain(0.35)
        .start_at(i as f32 * 0.07)
    });
    arpeggio
        .fold(Sound::new("powerup"), Sound::layer)
        .reverb(0.55, 0.3, 0.28)
        .peak_db(-3.0)
}

fn explosion() -> Sound {
    let rumble = Layer::new(BrownNoise, noise(), Adsr::new(0.005, 0.9, 0.0, 0.0, 0.0))
        .filter(LowPass, Sweep::new(2500.0, 120.0, 0.8), 0.2)
        .gain(1.0);
    let boom = Layer::new(
        Sine,
        Sweep::new(90.0, 30.0, 0.4),
        Adsr::percussive(0.002, 0.5),
    )
    .gain(0.8);
    let crack = Layer::new(WhiteNoise, noise(), Adsr::percussive(0.0, 0.06))
        .filter(HighPass, Sweep::constant(2500.0), 0.2)
        .gain(0.45);
    Sound::new("explosion")
        .layer(rumble)
        .layer(boom)
        .layer(crack)
        .drive(2.0)
        .reverb(0.6, 0.6, 0.22)
        .peak_db(-1.0)
}

fn death() -> Sound {
    let fall = Layer::new(
        Saw,
        Sweep::new(440.0, 70.0, 0.7),
        Adsr::new(0.005, 0.1, 0.7, 0.45, 0.25),
    )
    .filter(LowPass, Sweep::new(2200.0, 250.0, 0.7), 0.4)
    .unison(2, 18.0)
    .vibrato(7.0, 0.3)
    .gain(0.6);
    let hit = Layer::new(
        Sine,
        Sweep::new(150.0, 40.0, 0.12),
        Adsr::percussive(0.001, 0.25),
    )
    .gain(0.9);
    Sound::new("death")
        .layer(fall)
        .layer(hit)
        .reverb(0.65, 0.4, 0.3)
        .peak_db(-2.0)
}

fn ui_click() -> Sound {
    let tick = Layer::new(
        Sine,
        Sweep::new(1900.0, 1200.0, 0.02),
        Adsr::percussive(0.001, 0.035),
    )
    .gain(0.7);
    let snap = Layer::new(WhiteNoise, noise(), Adsr::percussive(0.0, 0.012))
        .filter(HighPass, Sweep::constant(4000.0), 0.2)
        .gain(0.25);
    Sound::new("ui_click")
        .layer(tick)
        .layer(snap)
        .peak_db(-13.0)
}

fn ui_confirm() -> Sound {
    let first = Layer::new(
        Sine,
        Sweep::constant(midi(79)),
        Adsr::percussive(0.002, 0.12),
    )
    .gain(0.6);
    let second = Layer::new(
        Sine,
        Sweep::constant(midi(86)),
        Adsr::percussive(0.002, 0.22),
    )
    .start_at(0.06)
    .gain(0.7);
    Sound::new("ui_confirm")
        .layer(first)
        .layer(second)
        .reverb(0.25, 0.4, 0.15)
        .peak_db(-7.0)
}

// ---------- Transitions musicales et stingers ----------

/// Note de saw large (unisson) filtrée : base des accords et arpèges.
fn saw_note(note: u8, start: f32, envelope: Adsr, cutoff: Sweep, gain: f32) -> Layer {
    Layer::new(Saw, Sweep::constant(midi(note)), envelope)
        .filter(LowPass, cutoff, 0.3)
        .unison(3, 14.0)
        .gain(gain)
        .start_at(start)
}

fn riser() -> Sound {
    let air = Layer::new(WhiteNoise, noise(), Adsr::new(1.9, 0.1, 0.0, 0.0, 0.0))
        .filter(BandPass, Sweep::new(400.0, 9000.0, 2.0), 0.55)
        .gain(0.6);
    let tone = Layer::new(
        Saw,
        Sweep::new(110.0, 440.0, 2.0),
        Adsr::new(1.8, 0.2, 0.0, 0.0, 0.0),
    )
    .filter(LowPass, Sweep::new(300.0, 7000.0, 2.0), 0.35)
    .unison(5, 25.0)
    .gain(0.5);
    Sound::new("riser")
        .layer(air)
        .layer(tone)
        .reverb(0.6, 0.4, 0.3)
        .peak_db(-2.0)
}

fn downlifter() -> Sound {
    let air = Layer::new(WhiteNoise, noise(), Adsr::percussive(0.01, 1.5))
        .filter(BandPass, Sweep::new(7000.0, 250.0, 1.5), 0.5)
        .gain(0.8);
    let drop = Layer::new(
        Sine,
        Sweep::new(220.0, 45.0, 1.2),
        Adsr::percussive(0.005, 1.3),
    )
    .gain(0.5);
    Sound::new("downlifter")
        .layer(air)
        .layer(drop)
        .reverb(0.55, 0.4, 0.3)
        .peak_db(-3.0)
}

fn stinger_victory() -> Sound {
    let pluck = Adsr::new(0.005, 0.15, 0.3, 0.05, 0.2);
    let held = Adsr::new(0.01, 0.4, 0.5, 0.5, 0.8);
    let arpeggio = [72, 76, 79]
        .iter()
        .enumerate()
        .map(|(i, &note)| saw_note(note, i as f32 * 0.09, pluck, Sweep::constant(3500.0), 0.3));
    let chord = [72, 76, 79, 84]
        .iter()
        .map(|&note| saw_note(note, 0.3, held, Sweep::new(1500.0, 4000.0, 0.4), 0.25));
    let bass = Layer::new(Triangle, Sweep::constant(midi(48)), held)
        .gain(0.5)
        .start_at(0.3);
    arpeggio
        .chain(chord)
        .fold(Sound::new("stinger_victory"), Sound::layer)
        .layer(bass)
        .reverb(0.65, 0.35, 0.3)
        .peak_db(-2.0)
}

fn stinger_boss() -> Sound {
    let hit = Layer::new(
        Sine,
        Sweep::new(70.0, 28.0, 0.4),
        Adsr::percussive(0.002, 0.9),
    );
    let rumble = Layer::new(BrownNoise, noise(), Adsr::percussive(0.003, 1.0))
        .filter(LowPass, Sweep::new(1500.0, 100.0, 0.8), 0.2)
        .gain(0.8);
    // Do, Do#, Fa# : seconde mineure + triton, l'accord le plus inquiétant qui soit.
    let cluster_envelope = Adsr::new(0.05, 0.3, 0.6, 0.8, 1.0);
    let cluster = [36, 37, 42].iter().map(|&note| {
        saw_note(
            note,
            0.05,
            cluster_envelope,
            Sweep::new(900.0, 400.0, 2.0),
            0.3,
        )
    });
    cluster
        .fold(
            Sound::new("stinger_boss").layer(hit).layer(rumble),
            Sound::layer,
        )
        .drive(1.8)
        .reverb(0.7, 0.5, 0.3)
        .peak_db(-1.0)
}

// ---------- Objets (synthèse modale) ----------
//
// Le COUP est une rafale de bruit de quelques millisecondes : plus elle est courte et
// brillante, plus le maillet est dur. L'OBJET est le résonateur.

fn strike(attack: f32, decay: f32) -> Layer {
    Layer::new(WhiteNoise, noise(), Adsr::percussive(attack, decay))
}

fn wood_knock() -> Sound {
    let knock = strike(0.0005, 0.006)
        .filter(LowPass, Sweep::constant(3500.0), 0.2)
        .resonator(Resonator::new(Material::Wood));
    Sound::new("wood_knock")
        .layer(Layer {
            pitch: Sweep::constant(420.0),
            ..knock
        })
        .peak_db(-3.0)
}

fn stone_hit() -> Sound {
    let hit = strike(0.0003, 0.004).resonator(Resonator::new(Material::Stone).brightness(0.6));
    // Quelques grains qui s'effritent après le choc : c'est ce qui fait lire de la PIERRE
    // et non un bloc de bois très amorti.
    let grit = strike(0.0, 0.03)
        .filter(BandPass, Sweep::constant(2500.0), 0.3)
        .gain(0.25)
        .start_at(0.004);
    Sound::new("stone_hit")
        .layer(Layer {
            pitch: Sweep::constant(180.0),
            ..hit
        })
        .layer(grit)
        .peak_db(-2.0)
}

fn metal_clang() -> Sound {
    let clang = strike(0.0002, 0.003).resonator(Resonator::new(Material::Metal).brightness(0.6));
    Sound::new("metal_clang")
        .layer(Layer {
            pitch: Sweep::constant(310.0),
            ..clang
        })
        .peak_db(-3.0)
}

fn bell() -> Sound {
    let mallet = strike(0.001, 0.01)
        .filter(LowPass, Sweep::constant(2000.0), 0.2)
        .resonator(Resonator::new(Material::Bell));
    Sound::new("bell")
        .layer(Layer {
            pitch: Sweep::constant(midi(72)),
            ..mallet
        })
        .peak_db(-4.0)
}

fn glass_tap() -> Sound {
    let tap = strike(0.0002, 0.002)
        .filter(HighPass, Sweep::constant(2000.0), 0.2)
        .resonator(Resonator::new(Material::Glass));
    Sound::new("glass_tap")
        .layer(Layer {
            pitch: Sweep::constant(1320.0),
            ..tap
        })
        .peak_db(-6.0)
}

fn bone_knock() -> Sound {
    let knock = strike(0.0005, 0.008)
        .filter(LowPass, Sweep::constant(1800.0), 0.2)
        .resonator(Resonator::new(Material::Bone));
    Sound::new("bone_knock")
        .layer(Layer {
            pitch: Sweep::constant(260.0),
            ..knock
        })
        .peak_db(-3.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render;

    #[test]
    fn every_name_has_a_preset() {
        for name in names() {
            assert_eq!(by_name(name).expect(name).name, name);
        }
        assert!(by_name("inconnu").is_none());
        let mut unique: Vec<_> = names().collect();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), CATALOG.len());
    }

    #[test]
    fn midi_reference_notes() {
        assert!((midi(69) - 440.0).abs() < 1e-3);
        assert!((midi(81) - 880.0).abs() < 1e-2);
    }

    #[test]
    fn presets_render_short_and_clean() {
        for name in names() {
            let samples = render(&by_name(name).unwrap(), 1);
            let seconds = samples.len() as f32 / 44_100.0;
            assert!(seconds > 0.02 && seconds < 6.0, "{name} dure {seconds}s");
            assert!(
                samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{name}"
            );
        }
    }
}
