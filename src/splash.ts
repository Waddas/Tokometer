// Pixel-art mascot splash — canvas port of the firmware's splash.cpp:
// 20x20 palette-indexed frames, grouped by usage rate, rotating every 20s.
import { type MascotAnimation } from "./animations";
import { MASCOTS, DEFAULT_MASCOT, type MascotId } from "./mascots";

// GROUP_NAMES from splash.cpp, verbatim.
const GROUPS: string[][] = [
  ["expression sleep", "idle breathe", "idle blink", "expression wink"],
  ["idle look around", "work think", "work coding"],
  ["dance sway", "expression surprise", "dance bounce"],
  ["dance bounce dj", "dance sway dj", "dance djmix"],
];
const ROTATE_MS = 20_000;
const GRID = 20;

export class Splash {
  private ctx: CanvasRenderingContext2D;
  private cell: number;
  private anims: Record<string, MascotAnimation> = MASCOTS[DEFAULT_MASCOT];
  private group = 0;
  private animIdx = 0;
  private pinned: string | null = null;
  private frame = 0;
  private running = false;
  private frameTimer: ReturnType<typeof setTimeout> | null = null;
  private rotateTimer: ReturnType<typeof setInterval> | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
    this.cell = Math.floor(Math.min(canvas.width, canvas.height) / GRID);
  }

  /** Names in the current group this mascot actually has, falling back to a
   *  lower group (then any animation) so a partial mascot never renders blank. */
  private groupNames(): string[] {
    for (let g = this.group; g >= 0; g--) {
      const names = GROUPS[g].filter((n) => this.anims[n]);
      if (names.length) return names;
    }
    return Object.keys(this.anims);
  }

  private animation(): MascotAnimation {
    if (this.pinned && this.anims[this.pinned]) return this.anims[this.pinned];
    const names = this.groupNames();
    return this.anims[names[this.animIdx % names.length]];
  }

  /** All animation names for the current mascot. */
  animationNames(): string[] {
    return Object.keys(this.anims);
  }

  /** Pin a specific animation regardless of usage rate; null resumes the
   *  automatic rate-grouped rotation. Dev/screenshot aid. */
  setAnimation(name: string | null): void {
    this.pinned = name;
    this.frame = 0;
    if (this.running) this.restartFrameLoop();
  }

  setGroup(group: number): void {
    if (group === this.group) return;
    this.group = group;
    this.animIdx = 0;
    this.frame = 0;
    if (this.running) this.restartFrameLoop();
  }

  setMascot(id: MascotId): void {
    if (this.anims === MASCOTS[id]) return;
    this.anims = MASCOTS[id];
    this.frame = 0;
    if (this.running) this.restartFrameLoop();
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    this.restartFrameLoop();
    this.rotateTimer = setInterval(() => this.rotate(), ROTATE_MS);
  }

  stop(): void {
    this.running = false;
    if (this.frameTimer) clearTimeout(this.frameTimer);
    if (this.rotateTimer) clearInterval(this.rotateTimer);
    this.frameTimer = null;
    this.rotateTimer = null;
  }

  private rotate(): void {
    if (this.pinned) return;
    this.animIdx = (this.animIdx + 1) % this.groupNames().length;
    this.frame = 0;
    this.restartFrameLoop();
  }

  private restartFrameLoop(): void {
    if (this.frameTimer) clearTimeout(this.frameTimer);
    this.drawFrame();
  }

  private drawFrame(): void {
    if (!this.running) return;
    const anim = this.animation();
    const fr = anim.frames[this.frame % anim.frames.length];

    const { ctx, canvas, cell } = this;
    const offX = Math.floor((canvas.width - cell * GRID) / 2);
    const offY = Math.floor((canvas.height - cell * GRID) / 2);
    // Transparent clear: the panel-chip backdrop comes from CSS.
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    for (let y = 0; y < GRID; y++) {
      for (let x = 0; x < GRID; x++) {
        const color = anim.palette[parseInt(fr.grid[y * GRID + x], 36)];
        if (!color || color === "transparent") continue;
        ctx.fillStyle = color;
        ctx.fillRect(offX + x * cell, offY + y * cell, cell, cell);
      }
    }

    this.frame = (this.frame + 1) % anim.frames.length;
    this.frameTimer = setTimeout(() => this.drawFrame(), fr.hold);
  }
}
