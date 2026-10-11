"""Exact tempo, beat phase and per-8-beat loudness (total and bass) of a track."""
import subprocess, sys
import numpy as np

SR, HOP = 22050, 128
path, lo, hi = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"], capture_output=True).stdout
x = np.frombuffer(raw, np.float32)
n = 1024
fr = np.lib.stride_tricks.sliding_window_view(x, n)[::HOP] * np.hanning(n)
mag = np.log1p(np.abs(np.fft.rfft(fr, axis=1)) * 10)
flux = np.maximum(0, np.diff(mag, axis=0)).sum(1)
fps = SR / HOP
best = None
for bpm in np.arange(lo, hi, 0.02):
    p = 60 / bpm
    for ph in np.arange(0, p, 0.004):
        s = flux[(np.arange(ph, min(len(x) / SR - 1, 90), p) * fps).astype(int)].sum()
        if best is None or s > best[0]:
            best = (s, bpm, ph)
_, bpm, ph = best
p = 60 / bpm
print(f"bpm {bpm:.2f} phase {ph:.3f} beat {p:.4f}")
bass = np.convolve(x, np.ones(200) / 200, mode="same")
k = 0
while ph + k * p < len(x) / SR - 1:
    a, b = ph + k * p, ph + (k + 8) * p
    seg, low = x[int(a * SR):int(b * SR)], bass[int(a * SR):int(b * SR)]
    r, l = np.sqrt(np.mean(seg ** 2)), np.sqrt(np.mean(low ** 2))
    print(f"beat {k:4d} {a:7.2f}s  rms {r:.3f} {'#' * int(r * 80):30s} bass {l:.3f} {'#' * int(l * 200)}")
    k += 8
