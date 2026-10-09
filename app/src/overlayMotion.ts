export type OverlayPhase = "closed" | "opening" | "open" | "closing";

// A later open/close must always win over an animation that is still finishing.
export class OverlayMotion {
  private revision = 0;
  private wantedVisible = false;

  constructor(
    private phase: (phase: OverlayPhase) => void,
    private hide: () => Promise<void>,
    private restore: () => Promise<void>,
    private delay: () => Promise<void>,
  ) {}

  get visible() {
    return this.wantedVisible;
  }
  open() {
    this.wantedVisible = true;
    const revision = ++this.revision;
    this.phase("opening");
    return revision;
  }
  isCurrent(revision: number) {
    return this.wantedVisible && revision === this.revision;
  }
  finishOpen(revision: number) {
    if (this.isCurrent(revision)) this.phase("open");
  }
  async close() {
    this.wantedVisible = false;
    const revision = ++this.revision;
    this.phase("closing");
    await this.delay();
    if (revision !== this.revision) return false;
    await this.hide();
    if (revision !== this.revision) {
      if (this.wantedVisible) await this.restore();
      return false;
    }
    this.phase("closed");
    return true;
  }
}
