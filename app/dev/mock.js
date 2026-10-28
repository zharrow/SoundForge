// Banc d'essai de l'interface dans un navigateur, sans Tauri : un faux côté natif qui renvoie un vrai rendu
// du moteur (preset « impact », préparé par `npm run banc`) pour toute prise, et une recette d'exemple.
// Sert à vérifier la mise en page et les gestes quand on ne voit pas l'app (Claude, un écran verrouillé) :
// npm run banc, puis http://localhost:1499/dev/index.html.
(() => {
  const callbacks = new Map();
  let next = 1;
  const b64 = async (url) => {
    const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
    let s = "";
    for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(s);
  };
  const layer = (over) => ({ waveform: "sine", pitch: { start: 190, end: 60, time: 0.07 }, envelope: { attack: 0.001, decay: 0.12, sustain: 0, hold: 0, release: 0 }, filter: null, vibrato: null, unison: { voices: 1, detune_cents: 0 }, gain: 0.9, start: 0, resonator: null, ...over });
  const recipe = {
    generator: "sound-forge (banc)", preset: null, variants: 3, amount: 0.3, seed: 121,
    sound: { name: "exemple", peak_db: -1, effects: { drive: 1.5, echo: null, reverb: { room: 0.25, damping: 0.5, mix: 0.08 } }, layers: [
      layer({}),
      layer({ waveform: "white_noise", pitch: { start: 1, end: 1, time: 0 }, envelope: { attack: 0, decay: 0.05, sustain: 0, hold: 0, release: 0 }, filter: { mode: "low_pass", cutoff: { start: 6000, end: 700, time: 0.05 }, resonance: 0.3 }, gain: 0.7 }),
      layer({ waveform: "white_noise", pitch: { start: 420, end: 420, time: 0 }, envelope: { attack: 0.001, decay: 0.008, sustain: 0, hold: 0, release: 0 }, resonator: { material: "wood", decay: 0.7, brightness: 0.55, dry: 0.1 }, gain: 0.7 }),
    ] },
  };
  const assets = Promise.all([b64("/dev/banc/impact.wav"), b64("/dev/banc/apercu.png"), recipe]);
  const envLength = (e) => e.attack + e.decay + e.hold + e.release;
  window.__mockCalls = [];
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
    transformCallback(fn) {
      const id = next++;
      callbacks.set(id, fn);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    convertFileSrc: (p) => p,
    async invoke(cmd, args) {
      window.__mockCalls.push(cmd);
      const [wav, png, recipe] = await assets;
      switch (cmd) {
        case "render":
          return {
            wav, spectrum: png, overview: args.overview ? png : null, duration: 0.68, peak_db: -1, rms_db: -17.8,
            bands: [["sub", 0.01], ["basses", 0.88], ["bas-médiums", 0], ["médiums", 0], ["haut-médiums", 0.05], ["présence", 0.04], ["air", 0.01]],
            layers: args.take.sound.layers.map((l) => [l.start, l.start + envLength(l.envelope)]),
            count: Math.max(1, args.take.variants), suffix: args.take.variants > 1 ? `_0${args.variant + 1}` : "",
          };
        case "library":
          return {
            folder: "/Users/essai/GameAssets/sfx",
            sounds: ["sfx_eku_saut", "sfx_lames_impact_corail", "sfx_resonance_echo"].map((name) => ({ name, editable: true, variants: 3, files: [], preset: null, generator: "sound-forge 0.5.0", modified: 0 })),
            presets: [["jump", "Saut léger"], ["impact", "Coup qui touche"]],
            recordings: [["voix/essai_claude", 0.7], ["voix/souffle", 1.2]],
          };
        case "load_recipe":
          return { name: args.name, ...recipe };
        case "preset":
          return recipe.sound;
        case "save":
          return { files: [`${args.name}_01.wav`] };
        case "exists":
          return false;
        case "save_recording":
          return `voix/${args.name}`;
        case "recording_wav":
          return wav;
        case "music_library":
          return { pieces: [{ name: "mus_exemple", bpm: 96, bars: 4, tracks: 2, modified: 0 }], instruments: [
            { id: "piano", name: "Piano droit", family: "claviers", kind: "frappe", range: [24, 103], layers: 3, kit: [] },
            { id: "harpe", name: "Harpe", family: "cordes", kind: "frappe", range: [28, 101], layers: 3, kit: [] },
            { id: "violons", name: "Violons (section) · tenu", family: "cordes", kind: "tenu", range: [48, 93], layers: 2, kit: [] },
            { id: "percussions", name: "Percussions d'orchestre (kit)", family: "percussions", kind: "frappe", range: [36, 55], layers: 7, kit: [36, 38, 40, 41, 43] },
          ], catalogue_error: null };
        case "load_music":
          return { generator: "banc", seed: 7, files: [], music: { name: "mus_exemple", bpm: 96, beats_per_bar: 4, bars: 4, looping: true, peak_db: -1.5,
            reverb: { room: 0.7, damping: 0.5, mix: 0.3 }, patterns: { eku: [{ at: 0, len: 2, note: "D5", vel: 0.6 }, { at: 2, len: 1, note: "F5", vel: 0.5 }, { at: 3, len: 1, note: "E5", vel: 0.5 }, { at: 4, len: 4, note: "C5", vel: 0.45 }] },
            tracks: [
              { name: "harpe", stem: "ambience", instrument: "harpe", volume_db: 0, pan: -0.3, reverb: 0.4, transpose: 0, humanize: 0.3,
                notes: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15].map((k) => ({ at: k, len: 1.5, note: [50, 57, 64, 65, 69, 65, 64, 57][k % 8], vel: 0.45 })), play: [] },
              { name: "piano", stem: "melody", instrument: "piano", volume_db: 4, pan: 0.15, reverb: 0.5, transpose: 0, humanize: 0.4, notes: [], play: [{ pattern: "eku", at: 8 }] },
            ] } };
        case "render_music_take":
          return { stems: args.music.tracks.map((t) => [t.stem || t.name, -24]).filter((v, i, a) => a.findIndex((w) => w[0] === v[0]) === i), duration: 10, frames: 441000, rendered: 1, spectrum: png };
        case "music_stem":
        case "preview_note":
          return Uint8Array.from(atob(wav), (c) => c.charCodeAt(0)).buffer;
        case "save_music":
          return [`${args.name}_mix.wav`];
        case "music_exists":
          return false;
        case "plugin:dialog|ask":
        case "plugin:dialog|message":
          return true;
        default:
          return null; // écouteurs d'événements, titre de fenêtre, pont prêt…
      }
    },
  };
})();
