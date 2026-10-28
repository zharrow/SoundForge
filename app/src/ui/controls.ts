/**
 * Contrôles de l'éditeur. Le champ numérique est le cœur de l'outil : un curseur à l'échelle de
 * l'oreille (logarithmique pour les fréquences, quadratique pour les durées courtes), un nombre
 * modifiable au clavier, et une étiquette qu'on glisse pour régler finement (⇧ : dix fois plus fin).
 * Double-clic sur l'étiquette : valeur par défaut.
 *
 * Un geste (glisser, saisir) appelle onChange avec sa phase : "start", "move"… puis "end". L'éditeur en
 * fait une seule étape d'historique.
 */

import { esc } from "./icons";

export type Scale = "lin" | "log" | "sq";
export type Phase = "start" | "move" | "end";

export interface FieldSpec {
  label: string;
  value: number;
  /** Bornes du curseur ; une valeur saisie peut en sortir, dans les limites `floor` et `ceil`. */
  min: number;
  max: number;
  scale?: Scale;
  integer?: boolean;
  unit?: string;
  hint?: string;
  floor?: number;
  ceil?: number;
  reset?: number;
  /** Texte d'appoint calculé (note d'une fréquence…). */
  extra?: (value: number) => string;
  onChange: (value: number, phase: Phase) => void;
}

const STEPS = 1000;

function toPosition(spec: FieldSpec, value: number): number {
  const { min, max } = spec;
  let p: number;
  switch (spec.scale ?? "lin") {
    case "log":
      p = Math.log(Math.max(value, min * 1e-3) / min) / Math.log(max / min);
      break;
    case "sq":
      p = Math.sqrt(Math.max(0, (value - min) / (max - min)));
      break;
    default:
      p = (value - min) / (max - min);
  }
  return Math.round(Math.min(1, Math.max(0, p)) * STEPS);
}

function fromPosition(spec: FieldSpec, position: number): number {
  const p = Math.min(1, Math.max(0, position / STEPS));
  const { min, max } = spec;
  let v: number;
  switch (spec.scale ?? "lin") {
    case "log":
      v = min * Math.pow(max / min, p);
      break;
    case "sq":
      v = min + (max - min) * p * p;
      break;
    default:
      v = min + (max - min) * p;
  }
  return tidy(spec, v);
}

/** Arrondi lisible : entier, ou 3 à 4 chiffres significatifs. */
function tidy(spec: FieldSpec, v: number): number {
  if (spec.integer) return Math.round(v);
  const magnitude = Math.abs(v);
  const decimals = magnitude >= 1000 ? 0 : magnitude >= 100 ? 1 : magnitude >= 10 ? 2 : magnitude >= 1 ? 3 : 4;
  return Number(v.toFixed(decimals));
}

export function formatNumber(v: number, integer = false): string {
  if (integer) return String(Math.round(v));
  const magnitude = Math.abs(v);
  const decimals = magnitude >= 1000 ? 0 : magnitude >= 100 ? 1 : magnitude >= 10 ? 2 : 3;
  return v.toFixed(decimals).replace(/\.?0+$/, "").replace(".", ",").replace(/^-0$/, "0") || "0";
}

function clampTyped(spec: FieldSpec, v: number): number {
  const floor = spec.floor ?? (spec.scale === "log" ? 0.001 : -Infinity);
  return tidy(spec, Math.min(spec.ceil ?? Infinity, Math.max(floor, v)));
}

export function numberField(spec: FieldSpec): HTMLElement {
  const root = document.createElement("div");
  root.className = "nf";
  if (spec.hint) root.title = spec.hint;
  root.innerHTML = `
    <span class="nf-label">${esc(spec.label)}</span>
    <input class="nf-range" type="range" min="0" max="${STEPS}" step="1">
    <input class="nf-num" type="text" inputmode="decimal" spellcheck="false">
    <span class="nf-unit">${esc(spec.unit ?? "")}</span>
    ${spec.extra ? '<span class="nf-extra"></span>' : ""}`;
  const label = root.querySelector<HTMLElement>(".nf-label")!;
  const range = root.querySelector<HTMLInputElement>(".nf-range")!;
  const num = root.querySelector<HTMLInputElement>(".nf-num")!;
  const extra = root.querySelector<HTMLElement>(".nf-extra");
  let value = spec.value;
  let active = false;

  const show = (v: number, fromRange = false) => {
    value = v;
    if (!fromRange) range.value = String(toPosition(spec, v));
    if (document.activeElement !== num) num.value = formatNumber(v, spec.integer);
    if (extra) extra.textContent = spec.extra!(v);
  };
  const emit = (v: number, phase: Phase) => {
    show(v, phase === "move");
    spec.onChange(v, phase);
  };
  show(value);

  range.addEventListener("input", () => {
    const v = fromPosition(spec, Number(range.value));
    if (!active) {
      active = true;
      spec.onChange(v, "start");
    }
    emit(v, "move");
  });
  range.addEventListener("change", () => {
    if (!active) return;
    active = false;
    emit(fromPosition(spec, Number(range.value)), "end");
  });

  const commitTyped = () => {
    const v = Number(num.value.replace(",", ".").replace(/\s/g, ""));
    if (!Number.isFinite(v) || num.value.trim() === "") {
      num.value = formatNumber(value, spec.integer);
      return;
    }
    const clamped = clampTyped(spec, v);
    if (clamped === value) {
      num.value = formatNumber(value, spec.integer);
      return;
    }
    spec.onChange(clamped, "start");
    emit(clamped, "end");
    num.value = formatNumber(clamped, spec.integer);
  };
  num.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      commitTyped();
      num.select();
    } else if (e.key === "Escape") {
      num.value = formatNumber(value, spec.integer);
      num.blur();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const delta = (e.key === "ArrowUp" ? 1 : -1) * (e.shiftKey ? 10 : 1) * (spec.integer ? 1 : 5);
      const v = fromPosition(spec, toPosition(spec, value) + delta);
      spec.onChange(v, "start");
      emit(v, "end");
      num.value = formatNumber(v, spec.integer);
    }
  });
  num.addEventListener("blur", commitTyped);
  num.addEventListener("focus", () => num.select());

  // Glisser l'étiquette : 300 px parcourent tout le curseur, ⇧ dix fois plus fin.
  label.addEventListener("pointerdown", (e) => {
    label.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startPos = toPosition(spec, value);
    let moved = false;
    label.onpointermove = (m) => {
      const pos = startPos + ((m.clientX - startX) / 300) * STEPS * (m.shiftKey ? 0.1 : 1);
      const v = fromPosition(spec, pos);
      if (!moved) {
        moved = true;
        spec.onChange(v, "start");
      }
      emit(v, "move");
    };
    label.onpointerup = () => {
      label.onpointermove = null;
      label.onpointerup = null;
      if (moved) emit(value, "end");
    };
  });
  label.addEventListener("dblclick", () => {
    if (spec.reset === undefined || spec.reset === value) return;
    spec.onChange(spec.reset, "start");
    emit(spec.reset, "end");
  });
  return root;
}

export function selectField(label: string, options: [string, string][], value: string, onChange: (value: string) => void): HTMLElement {
  const root = document.createElement("label");
  root.className = "sf";
  root.innerHTML = `<span class="nf-label">${esc(label)}</span><select>${options
    .map(([v, text]) => `<option value="${esc(v)}"${v === value ? " selected" : ""}>${esc(text)}</option>`)
    .join("")}</select>`;
  root.querySelector("select")!.addEventListener("change", (e) => onChange((e.target as HTMLSelectElement).value));
  return root;
}

export function segmented(options: [string, string][], value: string, onChange: (value: string) => void): HTMLElement {
  const root = document.createElement("div");
  root.className = "segmented";
  root.innerHTML = options
    .map(([v, text]) => `<button type="button" data-value="${esc(v)}" class="${v === value ? "on" : ""}">${esc(text)}</button>`)
    .join("");
  root.addEventListener("click", (e) => {
    const v = (e.target as HTMLElement).closest<HTMLElement>("[data-value]")?.dataset.value;
    if (v && v !== value) onChange(v);
  });
  return root;
}

/** Bloc titré, éventuellement activable (filtre, résonateur, effets…). Désactivé, son contenu est caché. */
export function block(title: string, body: HTMLElement[], toggle?: { on: boolean; onChange: (on: boolean) => void; hint?: string }, help?: string): HTMLElement {
  const root = document.createElement("section");
  root.className = `block${toggle && !toggle.on ? " off" : ""}`;
  const head = document.createElement("header");
  head.innerHTML = `<h3>${esc(title)}</h3>`;
  if (toggle) {
    const sw = document.createElement("button");
    sw.type = "button";
    sw.className = `switch${toggle.on ? " on" : ""}`;
    sw.title = toggle.hint ?? (toggle.on ? "Désactiver" : "Activer");
    sw.setAttribute("aria-pressed", String(toggle.on));
    sw.addEventListener("click", () => toggle.onChange(!toggle.on));
    head.append(sw);
    head.querySelector("h3")!.addEventListener("click", () => toggle.onChange(!toggle.on));
  }
  root.append(head);
  if (help) root.append(el("p", "block-help", esc(help)));
  if (!toggle || toggle.on) {
    const content = document.createElement("div");
    content.className = "block-body";
    content.append(...body);
    root.append(content);
  }
  return root;
}

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", html = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (html) node.innerHTML = html;
  return node;
}
