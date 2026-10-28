/**
 * L'aperçu du son rendu : forme d'onde (avec la tête de lecture), une barre par couche sur l'axe du temps
 * (de son départ à la fin de sa queue), le spectrogramme du moteur (le même que voit Claude) et les
 * mesures. Un clic dans l'aperçu écoute le son.
 */

import { peaks } from "../audio";
import type { Editor } from "../editor";
import { formatNumber } from "./controls";
import { esc } from "./icons";
import { layerColor } from "./layers";

export class Preview {
  private wave: HTMLCanvasElement;
  private bars: HTMLCanvasElement;
  private spectrum: HTMLImageElement;
  private head: HTMLElement;
  private info: HTMLElement;
  private empty: HTMLElement;

  constructor(
    private root: HTMLElement,
    private editor: Editor,
  ) {
    root.innerHTML = `
      <div class="preview-stage">
        <div class="preview-track">
          <canvas class="wave"></canvas>
          <canvas class="bars"></canvas>
          <img class="spectrum" alt="" draggable="false">
          <div class="axis"><span>20 kHz</span><span>1 kHz</span><span>100 Hz</span></div>
          <div class="playhead"></div>
        </div>
        <div class="preview-empty"></div>
      </div>
      <div class="preview-info"></div>`;
    this.wave = root.querySelector("canvas.wave")!;
    this.bars = root.querySelector("canvas.bars")!;
    this.spectrum = root.querySelector("img.spectrum")!;
    this.head = root.querySelector(".playhead")!;
    this.info = root.querySelector(".preview-info")!;
    this.empty = root.querySelector(".preview-empty")!;
    root.querySelector(".preview-track")!.addEventListener("click", () => editor.player.play());
    new ResizeObserver(() => this.render()).observe(root.querySelector(".preview-track")!);
    const tick = () => {
      const p = editor.player.position();
      this.head.style.opacity = p === null ? "0" : "1";
      if (p !== null) this.head.style.left = `${p * 100}%`;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  render(): void {
    const { rendered, renderError, player } = this.editor;
    this.root.classList.toggle("stale", this.editor.rendering);
    this.empty.textContent = renderError ?? "";
    this.empty.classList.toggle("visible", !!renderError);
    if (!rendered || !player.buffer) {
      this.info.innerHTML = "";
      return;
    }
    this.drawWave(player.buffer);
    this.drawBars(rendered.layers, rendered.duration);
    this.spectrum.src = `data:image/png;base64,${rendered.spectrum}`;
    const total = rendered.bands.reduce((sum, [, v]) => sum + v, 0) || 1;
    const dominant = rendered.bands.reduce((best, band) => (band[1] > best[1] ? band : best), rendered.bands[0]);
    this.info.innerHTML = `
      <span><b>${formatNumber(rendered.duration)}</b> s</span>
      <span>crête <b>${formatNumber(rendered.peak_db)}</b> dB</span>
      <span>RMS <b>${formatNumber(rendered.rms_db)}</b> dB</span>
      <span class="bands" title="Répartition de l'énergie par bande">${rendered.bands
        .map(([name, v]) => `<i style="--h:${Math.round((v / total) * 100)}%" title="${esc(name)} : ${Math.round((v / total) * 100)} %"></i>`)
        .join("")}</span>
      <span>dominante : <b>${esc(dominant?.[0] ?? "")}</b></span>
      ${this.editor.partial ? '<span class="partial">aperçu partiel : couches coupées</span>' : ""}`;
  }

  private drawWave(buffer: AudioBuffer): void {
    const canvas = this.wave;
    const ratio = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const h = Math.max(1, Math.round(canvas.clientHeight * ratio));
    canvas.width = w;
    canvas.height = h;
    const g = canvas.getContext("2d")!;
    g.clearRect(0, 0, w, h);
    const mid = h / 2;
    g.fillStyle = "rgba(255,255,255,0.06)";
    g.fillRect(0, mid, w, 1);
    const p = peaks(buffer, w);
    g.fillStyle = "#6ebeff";
    for (let x = 0; x < w; x++) {
      const lo = p[x * 2];
      const hi = p[x * 2 + 1];
      const top = mid - hi * mid * 0.95;
      const bottom = mid - lo * mid * 0.95;
      g.fillRect(x, top, 1, Math.max(1, bottom - top));
    }
  }

  private drawBars(layers: [number, number][], duration: number): void {
    const canvas = this.bars;
    const ratio = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const rowH = Math.round(5 * ratio);
    const gap = Math.round(2 * ratio);
    const visible = this.editor.doc.take.sound.layers.map((_, i) => i).filter((i) => this.editor.audible(i));
    const h = Math.max(1, visible.length * (rowH + gap));
    canvas.width = w;
    canvas.height = h;
    canvas.style.height = `${h / ratio}px`;
    const g = canvas.getContext("2d")!;
    g.clearRect(0, 0, w, h);
    const span = Math.max(duration, 0.001);
    // Les mesures du rendu ne portent que sur les couches entendues, dans l'ordre.
    visible.forEach((layerIndex, row) => {
      const [start, end] = layers[row] ?? [0, 0];
      g.fillStyle = layerColor(layerIndex);
      const x0 = (start / span) * w;
      const x1 = Math.max(x0 + 2, (Math.min(end, span) / span) * w);
      g.beginPath();
      g.roundRect(x0, row * (rowH + gap), x1 - x0, rowH, rowH / 2);
      g.fill();
    });
  }
}
