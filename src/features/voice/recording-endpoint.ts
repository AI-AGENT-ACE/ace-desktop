// Energy endpointing, not speech recognition. Time is measured in captured audio.
export class RecordingEndpoint {
  private elapsedMs = 0;
  private voicedMs = 0;
  private quietMs = 0;
  get hasSpeech() {
    return this.voicedMs >= 250;
  }
  feed(rms: number, durationMs: number): 'silence' | 'timeout' | null {
    this.elapsedMs += durationMs;
    if (rms >= 0.01) {
      this.voicedMs += durationMs;
      this.quietMs = 0;
    } else {
      this.quietMs += durationMs;
    }
    if (this.voicedMs >= 250 && this.quietMs >= 1500) return 'silence';
    if (this.elapsedMs >= (this.voicedMs >= 250 ? 30000 : 10000)) return 'timeout';
    return null;
  }
}
