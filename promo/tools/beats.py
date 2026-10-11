"""Tempo, beat phase and loudness envelope of an audio file (numpy only)."""
import subprocess, sys
import numpy as np

SR = 22050
HOP = 256


def load(path):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"],
        capture_output=True, check=True,
    ).stdout
    return np.frombuffer(raw, np.float32)


def onset_envelope(x):
    n = 1024
    frames = np.lib.stride_tricks.sliding_window_view(x, n)[::HOP] * np.hanning(n)
    mag = np.log1p(np.abs(np.fft.rfft(frames, axis=1)) * 10)
    flux = np.maximum(0, np.diff(mag, axis=0)).sum(axis=1)
    flux -= np.convolve(flux, np.ones(16) / 16, mode="same")
    return np.maximum(flux, 0)


def tempo(env):
    fps = SR / HOP
    env = env - env.mean()
    ac = np.correlate(env, env, mode="full")[len(env) - 1 :]
    lags = np.arange(len(ac))
    bpm = 60 * fps / np.maximum(lags, 1)
    ok = (bpm > 80) & (bpm < 180)
    best = lags[ok][np.argmax(ac[ok])]
    period = best / fps
    # Beat phase: offset that best aligns with onsets.
    offsets = np.arange(0, best)
    score = [env[o::best].sum() if o < len(env) else 0 for o in offsets]
    phase = offsets[int(np.argmax(score))] / fps
    return 60 / period, period, phase


def main():
    for path in sys.argv[1:]:
        x = load(path)
        env = onset_envelope(x)
        bpm, period, phase = tempo(env[: int(60 * SR / HOP)])
        # Loudness per 2 s.
        rms = [float(np.sqrt(np.mean(x[i : i + 2 * SR] ** 2))) for i in range(0, min(len(x), 70 * SR), 2 * SR)]
        bar = "".join(" .:-=+*#%@"[min(9, int(r / max(rms) * 9.99))] for r in rms)
        print(f"{path}: {len(x)/SR:6.1f}s  bpm {bpm:6.1f}  first beat {phase:5.2f}s  loud/2s [{bar}]")


main()
