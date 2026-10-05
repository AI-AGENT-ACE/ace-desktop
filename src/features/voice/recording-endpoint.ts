// Energy endpointing, not speech recognition. Time is measured in captured audio.
export class RecordingEndpoint {
  constructor(
    private initialSilenceMs = 10000,
    private minimumSpeechMs = 250,
  ) {}
  private elapsedMs = 0;
  private voicedMs = 0;
  private quietMs = 0;
  get hasSpeech() {
    return this.voicedMs >= this.minimumSpeechMs;
  }
  feed(rms: number, durationMs: number): 'silence' | 'timeout' | null {
    this.elapsedMs += durationMs;
    if (rms >= 0.01) {
      this.voicedMs += durationMs;
      this.quietMs = 0;
    } else {
      this.quietMs += durationMs;
    }
    if (this.hasSpeech && this.quietMs >= 1500) return 'silence';
    // A voice beginning just before the retry deadline gets enough time to qualify.
    if (
      this.elapsedMs >= (this.hasSpeech ? 30000 : this.initialSilenceMs) &&
      (this.hasSpeech || this.voicedMs === 0 || this.quietMs >= this.minimumSpeechMs)
    )
      return 'timeout';
    return null;
  }
}
