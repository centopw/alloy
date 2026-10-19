import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  ClipTooShort,
  downsamplePeaks,
  formatDuration,
  pickSupportedMimeType,
  VoiceRecorder,
} from './voiceRecorder';

describe('downsamplePeaks', () => {
  it('returns empty array when samples is empty or points <= 0', () => {
    expect(downsamplePeaks([], 64)).toEqual([]);
    expect(downsamplePeaks([1, 2, 3], 0)).toEqual([]);
    expect(downsamplePeaks([1, 2, 3], -5)).toEqual([]);
  });

  it('returns exact samples when samples length <= points', () => {
    const samples = [10, 50, 100, 20];
    expect(downsamplePeaks(samples, 4)).toEqual([10, 50, 100, 20]);
    expect(downsamplePeaks(samples, 10)).toEqual([10, 50, 100, 20]);
  });

  it('picks peak value in each bucket when samples length > points', () => {
    // 8 samples downsampled to 4 points -> buckets of 2
    const samples = [10, 80, 20, 15, 95, 30, 40, 60];
    const result = downsamplePeaks(samples, 4);
    expect(result).toEqual([80, 20, 95, 60]);
  });

  it('downsamples large sample sets to exactly points count', () => {
    const samples = Array.from({ length: 300 }, (_, i) => i % 1000);
    const result = downsamplePeaks(samples, 64);
    expect(result.length).toBe(64);
    for (const val of result) {
      expect(val).toBeGreaterThanOrEqual(0);
      expect(val).toBeLessThanOrEqual(1023);
    }
  });
});

describe('formatDuration', () => {
  it('formats zero and small durations', () => {
    expect(formatDuration(0)).toBe('0:00');
    expect(formatDuration(500)).toBe('0:00');
    expect(formatDuration(1000)).toBe('0:01');
    expect(formatDuration(9000)).toBe('0:09');
    expect(formatDuration(15000)).toBe('0:15');
  });

  it('formats minutes and seconds', () => {
    expect(formatDuration(65000)).toBe('1:05');
    expect(formatDuration(125000)).toBe('2:05');
    expect(formatDuration(900000)).toBe('15:00');
  });

  it('handles negative input gracefully', () => {
    expect(formatDuration(-1000)).toBe('0:00');
  });
});

describe('pickSupportedMimeType', () => {
  it('returns first supported mime type', () => {
    const originalMediaRecorder = globalThis.MediaRecorder;

    const mockIsTypeSupported = vi.fn((type: string) => type.includes('webm'));
    globalThis.MediaRecorder = {
      isTypeSupported: mockIsTypeSupported,
    } as unknown as typeof MediaRecorder;

    expect(pickSupportedMimeType()).toBe('audio/webm;codecs=opus');

    globalThis.MediaRecorder = originalMediaRecorder;
  });

  it('falls back through preferred mime types', () => {
    const originalMediaRecorder = globalThis.MediaRecorder;

    const mockIsTypeSupported = vi.fn((type: string) => type === 'audio/mp4');
    globalThis.MediaRecorder = {
      isTypeSupported: mockIsTypeSupported,
    } as unknown as typeof MediaRecorder;

    expect(pickSupportedMimeType()).toBe('audio/mp4');

    globalThis.MediaRecorder = originalMediaRecorder;
  });
});

describe('VoiceRecorder', () => {
  let mockStream: MediaStream;
  let mockTrack: MediaStreamTrack;
  let mockRecorderInstance: {
    start: ReturnType<typeof vi.fn>;
    stop: ReturnType<typeof vi.fn>;
    state: string;
    mimeType: string;
    ondataavailable: ((e: { data: Blob }) => void) | null;
    onstop: ((e: Event) => void) | null;
  };

  beforeEach(() => {
    vi.useFakeTimers();

    mockTrack = {
      stop: vi.fn(),
    } as unknown as MediaStreamTrack;

    mockStream = {
      getTracks: vi.fn(() => [mockTrack]),
    } as unknown as MediaStream;

    mockRecorderInstance = {
      start: vi.fn(() => {
        mockRecorderInstance.state = 'recording';
      }),
      stop: vi.fn(() => {
        mockRecorderInstance.state = 'inactive';
        if (mockRecorderInstance.onstop) {
          mockRecorderInstance.onstop(new Event('stop'));
        }
      }),
      state: 'inactive',
      mimeType: 'audio/webm',
      ondataavailable: null,
      onstop: null,
    };

    function MockMediaRecorder() {
      return mockRecorderInstance;
    }
    MockMediaRecorder.isTypeSupported = vi.fn(() => true);
    globalThis.MediaRecorder = MockMediaRecorder as unknown as typeof MediaRecorder;

    Object.defineProperty(globalThis.navigator, 'mediaDevices', {
      value: {
        getUserMedia: vi.fn(async () => mockStream),
      },
      writable: true,
      configurable: true,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('initializes in idle state', () => {
    const recorder = new VoiceRecorder();
    expect(recorder.state).toBe('idle');
    expect(recorder.elapsedMs).toBe(0);
    expect(recorder.levels).toEqual([]);
  });

  it('throws readable error when microphone permission is denied', async () => {
    const deniedErr = new Error('Permission denied');
    deniedErr.name = 'NotAllowedError';
    vi.mocked(navigator.mediaDevices.getUserMedia).mockRejectedValueOnce(deniedErr);

    const recorder = new VoiceRecorder();
    await expect(recorder.start()).rejects.toThrow('Microphone permission denied');
    expect(recorder.state).toBe('idle');
  });

  it('throws readable error when no microphone is found', async () => {
    const notFoundErr = new Error('Device not found');
    notFoundErr.name = 'NotFoundError';
    vi.mocked(navigator.mediaDevices.getUserMedia).mockRejectedValueOnce(notFoundErr);

    const recorder = new VoiceRecorder();
    await expect(recorder.start()).rejects.toThrow('No microphone found');
    expect(recorder.state).toBe('idle');
  });

  it('starts recording and tracks elapsed time', async () => {
    const recorder = new VoiceRecorder();
    await recorder.start();

    expect(recorder.state).toBe('recording');
    expect(mockRecorderInstance.start).toHaveBeenCalled();

    vi.advanceTimersByTime(350);
    expect(recorder.elapsedMs).toBeGreaterThanOrEqual(300);

    recorder.cancel();
    expect(recorder.state).toBe('idle');
  });

  it('rejects with ClipTooShort when stopped under 500 ms', async () => {
    const recorder = new VoiceRecorder();
    await recorder.start();

    vi.advanceTimersByTime(200);

    await expect(recorder.stop()).rejects.toThrow(ClipTooShort);
    expect(recorder.state).toBe('idle');
    expect(mockTrack.stop).toHaveBeenCalled();
  });

  it('successfully stops recording after 500 ms and returns VoiceClip', async () => {
    const recorder = new VoiceRecorder();
    await recorder.start();

    // Simulate audio chunk
    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({
        data: new Blob(['audio-data'], { type: 'audio/webm' }),
      });
    }

    vi.advanceTimersByTime(1200);

    const stopPromise = recorder.stop();
    const clip = await stopPromise;

    expect(clip).toBeDefined();
    expect(clip.mimeType).toBe('audio/webm');
    expect(clip.durationMs).toBeGreaterThanOrEqual(1200);
    expect(Array.isArray(clip.waveform)).toBe(true);
    expect(recorder.state).toBe('idle');
    expect(mockTrack.stop).toHaveBeenCalled();
  });

  it('cancels active recording and releases tracks', async () => {
    const recorder = new VoiceRecorder();
    await recorder.start();

    recorder.cancel();
    expect(recorder.state).toBe('idle');
    expect(mockTrack.stop).toHaveBeenCalled();
  });

  it('automatically stops at 15 minutes and calls onlimit callback', async () => {
    const recorder = new VoiceRecorder();
    const onlimit = vi.fn();
    await recorder.start(onlimit);

    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({
        data: new Blob(['audio-data'], { type: 'audio/webm' }),
      });
    }

    // Advance 15 minutes
    await vi.advanceTimersByTimeAsync(15 * 60 * 1000 + 100);

    expect(onlimit).toHaveBeenCalled();
  });
});
