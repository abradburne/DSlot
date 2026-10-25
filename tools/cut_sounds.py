#!/usr/bin/env python3
"""Cut the insert and eject clips from a recording of a DS card going in and out.

usage: cut_sounds.py RECORDING OUT_DIR [VARIANT]

Needs ffmpeg. Writes OUT_DIR/insert.pcm and OUT_DIR/eject.pcm (48 kHz, 16-bit, mono).
The click times below are for the recording the shipped sounds were made from.
"""
import math, struct, subprocess, sys

R = 48000
INSERT_CLICKS = (6.560, 6.785)
EJECT_CLICKS = (8.605, 8.870)

# name: (high-pass Hz, bass shelf dB, treble shelf dB, saturation drive, peak level)
VARIANTS = {
    "light": (180, 0, 0, 0.0, 12000),
    "between": (100, 5, -2, 1.0, 14000),
    "bass": (70, 10, -4, 1.6, 15000),
    "heavy": (50, 15, -6, 2.2, 15000),
}


def load(path, highpass, bass, treble):
    chain = f"highpass=f={highpass},bass=g={bass}:f=220:w=0.7,treble=g={treble}:f=5000"
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(R), "-af", chain, "-f", "s16le", "-"],
        check=True, capture_output=True,
    ).stdout
    return list(struct.unpack(f"<{len(raw) // 2}h", raw))


def onset(s, t0, t1):
    a, b = int(t0 * R), int(t1 * R)
    peak = max(abs(x) for x in s[a:b])
    return next(i for i in range(a, b) if abs(s[i]) > peak * 0.3)


def cut(s, clicks, drive, level, pre=0.025, post=0.16):
    floor = max(8, sorted(abs(x) for x in s[: R // 2])[int(R // 2 * 0.95)])
    first = onset(s, clicks[0] - 0.03, clicks[0] + 0.05)
    second = onset(s, clicks[1] - 0.03, clicks[1] + 0.06)
    seg = [float(x) for x in s[first - int(pre * R) : second + int(post * R)]]
    # Expander: pull the room noise down between and after the clicks before boosting.
    env, out = 0.0, []
    for x in seg:
        env = abs(x) if abs(x) > env else env * math.exp(-1 / (0.045 * R))
        out.append(x * min(1.0, (env / (floor * 5)) ** 1.5))
    if drive:
        peak = max(abs(x) for x in out)
        out = [math.tanh(drive * x / peak) for x in out]
    fade = int(0.004 * R)
    for i in range(fade):
        out[i] *= i / fade
        out[-1 - i] *= i / fade
    k = level / max(abs(x) for x in out)
    return struct.pack(f"<{len(out)}h", *[int(x * k) for x in out])


def main():
    recording, out_dir = sys.argv[1], sys.argv[2]
    highpass, bass, treble, drive, level = VARIANTS[sys.argv[3] if len(sys.argv) > 3 else "bass"]
    s = load(recording, highpass, bass, treble)
    for name, clicks in (("insert", INSERT_CLICKS), ("eject", EJECT_CLICKS)):
        with open(f"{out_dir}/{name}.pcm", "wb") as f:
            f.write(cut(s, clicks, drive, level))


if __name__ == "__main__":
    main()
