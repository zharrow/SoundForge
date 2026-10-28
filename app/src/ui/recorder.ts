/**
 * Enregistrer sa voix. Trois temps : on enregistre (10 s au plus, vumètre), on rogne (le silence autour
 * est retiré tout seul, deux poignées ajustent le début et la fin), on garde. La prise va dans
 * `voix/<nom>.wav` du dossier de travail ; elle devient la source d'une couche, transformable comme les
 * autres (transposition, filtre, résonateur, effets, variantes). Claude peut l'écouter par son nom
 * (analyze_sound, spectrogram « voix/<nom> ») et s'en servir dans ses sons.
 *
 * La capture garde le son brut : ni annulation d'écho, ni réduction de bruit, ni gain automatique, qui
 * déformeraient un bruit de bouche.
 */

import type { Editor } from "../editor";
import { SoundDoc } from "../doc";
import { newTake, newSound, sampleLayer, validName } from "../model";
import * as native from "../native";
import { el } from "./controls";
import { esc, icon } from "./icons";

const MAX_SECONDS = 10;
const SILENCE_DB = -42;

export function encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const text = (offset: number, s: string) => [...s].forEach((c, i) => view.setUint8(offset + i, c.charCodeAt(0)));
  text(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((s, i) => view.setInt16(44 + i * 2, Math.round(Math.max(-1, Math.min(1, s)) * 32767), true));
  return bytes;
}

export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/** Bornes du son utile : on retire le silence avant et après, en gardant une petite marge. */
export function trimBounds(data: Float32Array, sampleRate: number): [number, number] {
  let peak = 0;
  for (const s of data) peak = Math.max(peak, Math.abs(s));
  if (peak === 0) return [0, data.length];
  const threshold = peak * Math.pow(10, SILENCE_DB / 20);
  const margin = Math.round(0.03 * sampleRate);
  let first = data.findIndex((s) => Math.abs(s) > threshold);
  let last = data.length - 1;
  while (last > 0 && Math.abs(data[last]) <= threshold) last--;
  first = Math.max(0, first - margin);
  last = Math.min(data.length, last + margin);
  return [first, Math.max(first + 1, last)];
}

/** La prise rognée, avec de courts fondus pour éviter les clics. */
export function cut(data: Float32Array, from: number, to: number, sampleRate: number): Float32Array {
  const out = data.slice(from, to);
  const fade = Math.min(Math.round(0.005 * sampleRate), Math.floor(out.length / 2));
  for (let i = 0; i < fade; i++) {
    out[i] *= i / fade;
    out[out.length - 1 - i] *= i / fade;
  }
  return out;
}

function nextName(editor: Editor): string {
  const taken = new Set((editor.library?.recordings ?? []).map(([name]) => name.replace(/^voix\//, "")));
  let n = 1;
  while (taken.has(`voix_${n}`)) n++;
  return `voix_${n}`;
}

export function openRecorder(editor: Editor): void {
  const dialog = el("dialog", "dialog recorder");
  document.body.append(dialog);
  let stream: MediaStream | null = null;
  let context: AudioContext | null = null;
  let chunks: Float32Array[] = [];
  let level = 0;
  let data: Float32Array | null = null;
  let rate = 48000;
  let bounds: [number, number] = [0, 0];
  let started = 0;
  let timer = 0;
  let preview: AudioBufferSourceNode | null = null;

  const close = () => {
    stopCapture();
    preview?.stop();
    dialog.close();
    dialog.remove();
  };
  dialog.addEventListener("cancel", close);

  const stopCapture = () => {
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    window.clearInterval(timer);
    void context?.close();
    context = null;
  };

  const idle = (message = "") => {
    dialog.innerHTML = `
      <h2>${icon("mic", 16)} Enregistrer ta voix</h2>
      <p class="muted">Approche-toi du micro et fais le son. Pour un effet : une onomatopée courte (« pshh », « tchak », un souffle). Pour un personnage : quelques mots. ${MAX_SECONDS} s au plus.</p>
      <div class="rec-stage">
        <button class="rec-button" title="Commencer (Espace)"><span></span></button>
        <div class="rec-meter"><i></i></div>
        <span class="rec-time">0,0 s</span>
      </div>
      ${message ? `<p class="error">${esc(message)}</p>` : ""}
      <div class="dialog-actions"><button class="button" data-close>Fermer</button></div>`;
    dialog.querySelector("[data-close]")!.addEventListener("click", close);
    dialog.querySelector(".rec-button")!.addEventListener("click", () => void record());
  };

  const record = async () => {
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
    } catch {
      return idle("Pas d'accès au micro. Autorise Sound Forge dans Réglages Système › Confidentialité et sécurité › Microphone, puis réessaie.");
    }
    context = new AudioContext();
    rate = context.sampleRate;
    chunks = [];
    const source = context.createMediaStreamSource(stream);
    const processor = context.createScriptProcessor(4096, 1, 1);
    const mute = context.createGain();
    mute.gain.value = 0; // le traitement doit aller jusqu'à la sortie pour tourner, mais sans se faire entendre
    processor.onaudioprocess = (e) => {
      const input = e.inputBuffer.getChannelData(0);
      chunks.push(new Float32Array(input));
      let sum = 0;
      for (const s of input) sum += s * s;
      level = Math.sqrt(sum / input.length);
    };
    source.connect(processor);
    processor.connect(mute);
    mute.connect(context.destination);
    started = performance.now();
    dialog.querySelector(".rec-button")!.classList.add("on");
    dialog.querySelector(".rec-button")!.setAttribute("title", "Arrêter (Espace)");
    const button = dialog.querySelector<HTMLButtonElement>(".rec-button")!;
    button.onclick = () => finish();
    const meter = dialog.querySelector<HTMLElement>(".rec-meter i")!;
    const time = dialog.querySelector<HTMLElement>(".rec-time")!;
    timer = window.setInterval(() => {
      const seconds = (performance.now() - started) / 1000;
      const db = 20 * Math.log10(Math.max(level, 1e-5));
      meter.style.width = `${Math.max(0, Math.min(100, ((db + 60) / 60) * 100))}%`;
      time.textContent = `${seconds.toFixed(1).replace(".", ",")} s`;
      if (seconds >= MAX_SECONDS) finish();
    }, 50);
  };

  const finish = () => {
    if (!stream) return;
    stopCapture();
    const total = chunks.reduce((n, c) => n + c.length, 0);
    data = new Float32Array(total);
    let offset = 0;
    for (const c of chunks) {
      data.set(c, offset);
      offset += c.length;
    }
    bounds = trimBounds(data, rate);
    review();
  };

  const review = () => {
    const seconds = () => (bounds[1] - bounds[0]) / rate;
    dialog.innerHTML = `
      <h2>${icon("mic", 16)} Ta prise</h2>
      <p class="muted">Le silence autour est retiré. Tire les poignées pour ajuster le début et la fin.</p>
      <div class="rec-review"><canvas></canvas></div>
      <div class="rec-row">
        <button class="button" data-play>${icon("play", 13)} Écouter</button>
        <span class="rec-length"></span>
        <span class="spacer"></span>
        <button class="link" data-again>Recommencer</button>
      </div>
      <label class="rec-name">Nom <input type="text" spellcheck="false" value="${nextName(editor)}"></label>
      <p class="error"></p>
      <div class="dialog-actions">
        <button class="button" data-keep="none">Garder seulement</button>
        <button class="button" data-keep="layer">Ajouter au son ouvert</button>
        <button class="button primary" data-keep="new">Nouveau son avec cette voix</button>
      </div>`;
    const canvas = dialog.querySelector("canvas")!;
    const length = dialog.querySelector<HTMLElement>(".rec-length")!;
    const draw = () => {
      drawTake(canvas, data!, bounds);
      length.textContent = `${seconds().toFixed(2).replace(".", ",")} s`;
    };
    requestAnimationFrame(draw);
    let handle = -1;
    canvas.addEventListener("pointerdown", (e) => {
      const rect = canvas.getBoundingClientRect();
      const at = Math.round(((e.clientX - rect.left) / rect.width) * data!.length);
      handle = Math.abs(at - bounds[0]) < Math.abs(at - bounds[1]) ? 0 : 1;
      canvas.setPointerCapture(e.pointerId);
      const move = (m: PointerEvent) => {
        const x = Math.round(((m.clientX - rect.left) / rect.width) * data!.length);
        const v = Math.max(0, Math.min(data!.length, x));
        bounds = handle === 0 ? [Math.min(v, bounds[1] - 100), bounds[1]] : [bounds[0], Math.max(v, bounds[0] + 100)];
        draw();
      };
      move(e);
      canvas.onpointermove = move;
      canvas.onpointerup = () => (canvas.onpointermove = null);
    });
    dialog.querySelector("[data-play]")!.addEventListener("click", () => {
      preview?.stop();
      const ctx = new AudioContext({ sampleRate: rate });
      const buffer = ctx.createBuffer(1, bounds[1] - bounds[0], rate);
      buffer.copyToChannel(cut(data!, bounds[0], bounds[1], rate) as Float32Array<ArrayBuffer>, 0);
      preview = ctx.createBufferSource();
      preview.buffer = buffer;
      preview.connect(ctx.destination);
      preview.start();
    });
    dialog.querySelector("[data-again]")!.addEventListener("click", () => {
      idle();
      void record();
    });
    dialog.querySelectorAll<HTMLButtonElement>("[data-keep]").forEach((button) =>
      button.addEventListener("click", async () => {
        const name = dialog.querySelector<HTMLInputElement>(".rec-name input")!.value.trim();
        const error = dialog.querySelector<HTMLElement>(".error")!;
        if (!validName(name)) return void (error.textContent = "Nom : lettres, chiffres, _ et - uniquement.");
        if ((editor.library?.recordings ?? []).some(([n]) => n === `voix/${name}`)) {
          return void (error.textContent = `« ${name} » existe déjà : choisis un autre nom.`);
        }
        const take = cut(data!, bounds[0], bounds[1], rate);
        const file = await native.saveRecording(name, toBase64(encodeWav(take, rate)));
        await editor.refreshLibrary();
        const layer = sampleLayer(file, take.length / rate);
        if (button.dataset.keep === "new") {
          const sound = { ...newSound(name), layers: [layer], peak_db: -3 };
          if (!editor.doc.dirty || (await native.confirm("Le son ouvert a des modifications non enregistrées. Les abandonner ?", "Sound Forge", "Abandonner"))) {
            editor.open(new SoundDoc(newTake(sound), null, null, `Nouveau son : voix « ${name} »`));
          }
        } else if (button.dataset.keep === "layer") {
          editor.edit(`Voix « ${name} » ajoutée`, (t) => void t.sound.layers.push(layer));
        }
        close();
      }),
    );
  };

  dialog.addEventListener("keydown", (e) => {
    if (e.code !== "Space" || (e.target as HTMLElement).tagName === "INPUT") return;
    e.preventDefault();
    if (stream) finish();
    else if (!data) void record();
  });

  idle();
  dialog.showModal();
}

function drawTake(canvas: HTMLCanvasElement, data: Float32Array, [from, to]: [number, number]): void {
  const ratio = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
  const h = Math.max(1, Math.round(canvas.clientHeight * ratio));
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext("2d")!;
  g.fillStyle = "#121317";
  g.fillRect(0, 0, w, h);
  const x0 = (from / data.length) * w;
  const x1 = (to / data.length) * w;
  g.fillStyle = "rgba(255,154,74,0.08)";
  g.fillRect(x0, 0, x1 - x0, h);
  for (let x = 0; x < w; x++) {
    let peak = 0;
    const a = Math.floor((x * data.length) / w);
    const b = Math.floor(((x + 1) * data.length) / w);
    for (let i = a; i < b; i++) peak = Math.max(peak, Math.abs(data[i]));
    const bar = Math.max(1, peak * h * 0.9);
    g.fillStyle = x >= x0 && x <= x1 ? "#6ebeff" : "rgba(110,190,255,0.25)";
    g.fillRect(x, (h - bar) / 2, 1, bar);
  }
  g.fillStyle = "#ff9a4a";
  for (const x of [x0, x1]) g.fillRect(Math.round(x) - ratio, 0, 2 * ratio, h);
}
