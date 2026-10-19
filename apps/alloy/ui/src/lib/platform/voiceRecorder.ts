/**
 * Voice message recording with MediaRecorder and AnalyserNode.
 * Provides waveform sampling during recording and clip duration tracking.
 */

export interface VoiceClip {
  blob: Blob;
  mimeType: string;
  durationMs: number;
  waveform: number[];
}

export class ClipTooShort extends Error {
  constructor() {
    super('Voice recording is too short (minimum 500 ms)');
    this.name = 'ClipTooShort';
  }
}

/**
 * Downsamples an array of integer samples into at most `points` peaks.
 * If samples.length <= points, returns the samples directly.
 * Otherwise buckets samples evenly and picks the maximum within each bucket.
 */
export function downsamplePeaks(samples: readonly number[], points: number): number[] {
  if (points <= 0 || samples.length === 0) return [];
  if (samples.length <= points) return [...samples];

  const bucketSize = samples.length / points;
  const peaks: number[] = new Array(points);

  for (let i = 0; i < points; i++) {
    const start = Math.floor(i * bucketSize);
    const end = Math.min(samples.length, Math.floor((i + 1) * bucketSize));
    let max = 0;
    for (let j = start; j < end; j++) {
      const val = samples[j] ?? 0;
      if (val > max) max = val;
    }
    peaks[i] = max;
  }

  return peaks;
}

/**
 * Formats a duration in milliseconds into "m:ss" (e.g. "0:05", "1:23").
 */
export function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${m}:${s < 10 ? '0' : ''}${s}`;
}

const MAX_RECORDING_MS = 15 * 60 * 1000; // 15 minutes
const SAMPLE_INTERVAL_MS = 100;
const MIN_CLIP_DURATION_MS = 500;

const PREFERRED_MIME_TYPES = [
  'audio/webm;codecs=opus',
  'audio/ogg;codecs=opus',
  'audio/mp4',
] as const;

export function pickSupportedMimeType(): string {
  if (typeof MediaRecorder === 'undefined' || typeof MediaRecorder.isTypeSupported !== 'function') {
    return 'audio/webm;codecs=opus';
  }
  for (const mime of PREFERRED_MIME_TYPES) {
    if (MediaRecorder.isTypeSupported(mime)) {
      return mime;
    }
  }
  return '';
}

/**
 * Voice recording state manager.
 * Use in Svelte components to create Runes via $state if needed,
 * or bind the state/elapsedMs/levels properties directly.
 */
export class VoiceRecorder {
  state: 'idle' | 'recording' | 'stopping' = 'idle';
  elapsedMs: number = 0;
  levels: number[] = [];

  private stream: MediaStream | null = null;
  private recorder: MediaRecorder | null = null;
  private audioCtx: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private chunks: Blob[] = [];
  private rawSamples: number[] = [];
  private startTimestamp = 0;
  private elapsedTimerId: number | undefined;
  private sampleTimerId: number | undefined;
  private onlimitCallback?: (clip: VoiceClip) => void;

  async start(onlimit?: (clip: VoiceClip) => void): Promise<void> {
    if (this.state !== 'idle') return;

    this.onlimitCallback = onlimit;
    this.chunks = [];
    this.rawSamples = [];
    this.levels = [];
    this.elapsedMs = 0;

    let mediaStream: MediaStream;
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch (err: unknown) {
      const name = err instanceof Error ? err.name : '';
      if (name === 'NotAllowedError' || name === 'PermissionDeniedError') {
        throw new Error('Microphone permission denied');
      }
      if (name === 'NotFoundError' || name === 'DevicesNotFoundError') {
        throw new Error('No microphone found');
      }
      throw new Error('Microphone unavailable');
    }

    this.stream = mediaStream;

    const mimeType = pickSupportedMimeType();
    const recorderOptions: MediaRecorderOptions = mimeType ? { mimeType } : {};
    let rec: MediaRecorder;
    try {
      rec = new MediaRecorder(mediaStream, recorderOptions);
    } catch {
      rec = new MediaRecorder(mediaStream);
    }
    this.recorder = rec;

    rec.ondataavailable = (e: BlobEvent) => {
      if (e.data && e.data.size > 0) {
        this.chunks.push(e.data);
      }
    };

    if (typeof window !== 'undefined' && 'AudioContext' in window) {
      try {
        const ctx = new window.AudioContext();
        const source = ctx.createMediaStreamSource(mediaStream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 256;
        source.connect(analyser);
        this.audioCtx = ctx;
        this.analyser = analyser;
      } catch {
        // Continue even if audio analyser creation fails
      }
    }

    this.startTimestamp = Date.now();
    this.state = 'recording';
    rec.start(100);

    this.elapsedTimerId = window.setInterval(() => {
      if (this.state !== 'recording') return;
      const elapsed = Date.now() - this.startTimestamp;
      this.elapsedMs = elapsed;
      if (elapsed >= MAX_RECORDING_MS) {
        void this.handleLimitReached();
      }
    }, 100);

    const frequencyData = this.analyser ? new Uint8Array(this.analyser.frequencyBinCount) : null;
    this.sampleTimerId = window.setInterval(() => {
      if (this.state !== 'recording') return;
      let peak0to1023 = 0;
      if (this.analyser && frequencyData) {
        this.analyser.getByteFrequencyData(frequencyData);
        let maxVal = 0;
        for (let i = 0; i < frequencyData.length; i++) {
          const v = frequencyData[i] ?? 0;
          if (v > maxVal) maxVal = v;
        }
        peak0to1023 = Math.min(1023, Math.round((maxVal / 255) * 1023));
      }
      this.rawSamples.push(peak0to1023);
      const level0to1 = peak0to1023 / 1023;
      if (this.levels.length >= 40) {
        this.levels = [...this.levels.slice(1), level0to1];
      } else {
        this.levels = [...this.levels, level0to1];
      }
    }, SAMPLE_INTERVAL_MS);
  }

  private async handleLimitReached(): Promise<void> {
    try {
      const clip = await this.stop();
      this.onlimitCallback?.(clip);
    } catch {
      // Ignored if too short or failed
    }
  }

  async stop(): Promise<VoiceClip> {
    if (this.state !== 'recording') {
      throw new Error('Not recording');
    }
    this.state = 'stopping';
    clearInterval(this.elapsedTimerId);
    clearInterval(this.sampleTimerId);

    const { promise, resolve, reject } = Promise.withResolvers<VoiceClip>();
    const rec = this.recorder;
    const duration = Date.now() - this.startTimestamp;

    if (!rec) {
      this.releaseResources();
      this.state = 'idle';
      reject(new Error('MediaRecorder not initialized'));
      return promise;
    }

    if (duration < MIN_CLIP_DURATION_MS) {
      this.cancel();
      reject(new ClipTooShort());
      return promise;
    }

    rec.onstop = () => {
      const mimeType = rec.mimeType || this.chunks[0]?.type || 'audio/webm';
      const blob = new Blob(this.chunks, { type: mimeType });
      const waveform = downsamplePeaks(this.rawSamples, 64);
      this.releaseResources();
      this.state = 'idle';
      resolve({
        blob,
        mimeType,
        durationMs: duration,
        waveform,
      });
    };

    try {
      if (rec.state !== 'inactive') {
        rec.stop();
      } else {
        rec.onstop?.(new Event('stop'));
      }
    } catch (err) {
      this.releaseResources();
      this.state = 'idle';
      reject(err);
    }

    return promise;
  }

  cancel(): void {
    clearInterval(this.elapsedTimerId);
    clearInterval(this.sampleTimerId);
    if (this.recorder && this.recorder.state !== 'inactive') {
      try {
        this.recorder.stop();
      } catch {
        // Ignore stop errors on cancellation
      }
    }
    this.releaseResources();
    this.state = 'idle';
    this.chunks = [];
    this.rawSamples = [];
    this.levels = [];
    this.elapsedMs = 0;
  }

  destroy(): void {
    this.cancel();
  }

  private releaseResources(): void {
    if (this.stream) {
      for (const track of this.stream.getTracks()) {
        try {
          track.stop();
        } catch {
          // Ignore track stop errors
        }
      }
      this.stream = null;
    }
    if (this.audioCtx) {
      try {
        void this.audioCtx.close();
      } catch {
        // Ignore audio context close errors
      }
      this.audioCtx = null;
    }
    this.analyser = null;
    this.recorder = null;
  }
}
