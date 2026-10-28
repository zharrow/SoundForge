/**
 * Lecture du rendu : le WAV renvoyé par le moteur est décodé par WebAudio et joué tel quel, sans
 * rééchantillonnage audible (44,1 kHz). Une seule lecture à la fois ; en boucle, le nouveau rendu
 * prend la place de l'ancien à la boucle suivante, ce qui permet d'écouter une retouche en continu.
 */

export class Player {
  private context: AudioContext | null = null;
  private source: AudioBufferSourceNode | null = null;
  private startedAt = 0;
  /** Ce qui joue : le son courant, ou une autre version (variation, « avant »). */
  private playingBuffer: AudioBuffer | null = null;
  buffer: AudioBuffer | null = null;
  /** Vrai quand ce qui joue n'est pas le son courant. */
  other = false;
  loop = false;
  onState: (playing: boolean) => void = () => {};

  private get ctx(): AudioContext {
    this.context ??= new AudioContext({ sampleRate: 44100 });
    return this.context;
  }

  async load(wavBase64: string): Promise<AudioBuffer> {
    this.buffer = await this.decode(wavBase64);
    return this.buffer;
  }

  decode(wavBase64: string): Promise<AudioBuffer> {
    const bytes = Uint8Array.from(atob(wavBase64), (c) => c.charCodeAt(0));
    return this.ctx.decodeAudioData(bytes.buffer);
  }

  /** Joue une autre version une fois, sans remplacer le son courant. Rend sa durée (s). */
  playBuffer(buffer: AudioBuffer): number {
    this.start(buffer, false);
    this.other = true;
    this.onState(true);
    return buffer.duration;
  }

  get playing(): boolean {
    return this.source !== null;
  }

  /** Position de lecture du son courant, de 0 à 1 (null à l'arrêt, ou quand une autre version joue). */
  position(): number | null {
    if (!this.source || !this.playingBuffer || this.other) return null;
    const elapsed = this.ctx.currentTime - this.startedAt;
    const duration = this.playingBuffer.duration;
    return this.loop ? (elapsed % duration) / duration : Math.min(1, elapsed / duration);
  }

  play(): void {
    if (!this.buffer) return;
    this.start(this.buffer, this.loop);
    this.other = false;
    this.onState(true);
  }

  private start(buffer: AudioBuffer, loop: boolean): void {
    this.stop(false);
    void this.ctx.resume();
    const source = this.ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(this.ctx.destination);
    source.onended = () => {
      if (this.source !== source) return;
      if (loop) this.play();
      else this.stop();
    };
    // La boucle est rejouée à la main (onended) : chaque tour prend le dernier rendu.
    source.start();
    this.startedAt = this.ctx.currentTime;
    this.source = source;
    this.playingBuffer = buffer;
  }

  stop(notify = true): void {
    const source = this.source;
    this.source = null;
    if (source) {
      source.onended = null;
      try {
        source.stop();
      } catch {}
    }
    if (notify) this.onState(false);
  }

  toggle(): void {
    if (this.playing) this.stop();
    else this.play();
  }
}

/** Minimum et maximum par colonne, pour dessiner la forme d'onde. */
export function peaks(buffer: AudioBuffer, columns: number): Float32Array {
  const data = buffer.getChannelData(0);
  const out = new Float32Array(columns * 2);
  for (let x = 0; x < columns; x++) {
    const from = Math.floor((x * data.length) / columns);
    const to = Math.max(from + 1, Math.floor(((x + 1) * data.length) / columns));
    let lo = 0;
    let hi = 0;
    for (let i = from; i < to && i < data.length; i++) {
      if (data[i] < lo) lo = data[i];
      if (data[i] > hi) hi = data[i];
    }
    out[x * 2] = lo;
    out[x * 2 + 1] = hi;
  }
  return out;
}
