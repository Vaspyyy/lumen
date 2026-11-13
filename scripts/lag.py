#!/usr/bin/env python3
"""Reads a lag record (LUMEN_LAG, see src/lag.rs) and says where the time went.

    scripts/lag.py record.jsonl            the report
    scripts/lag.py before.jsonl after.jsonl   two records side by side

Times are from a key reaching lumen to the frame that shows the program's
answer being handed to the compositor; the compositor and the display add
about one refresh on top, the same for every build.
"""

import json
import sys
from bisect import bisect_left
from collections import Counter, defaultdict

# a glide of the cursor takes this long (GLIDE in src/element.rs)
GLIDE_MS = 70


def load(path):
    events = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError:
                    pass  # a line cut off when lumen was killed
    events.sort(key=lambda e: e["t"])
    return events


def pct(values, p):
    if not values:
        return None
    values = sorted(values)
    k = (len(values) - 1) * p / 100
    lo, hi = int(k), min(int(k) + 1, len(values) - 1)
    return values[lo] + (values[hi] - values[lo]) * (k - lo)


def ms(us):
    return None if us is None else us / 1000


def analyze(events):
    by = defaultdict(list)
    for e in events:
        by[e["e"]].append(e)
    frames = by["frame"]
    frame_starts = [f["t"] for f in frames]
    reads = by["read"]
    read_times = [r["t"] for r in reads]
    paints = by["paint"]
    refresh = pct([f["refresh"] for f in frames if f.get("refresh")], 50)

    def first_frame_after(t):
        i = bisect_left(frame_starts, t)
        return frames[i] if i < len(frames) else None

    # Each key: the program's first answer, the last one of its burst, and the frames that showed them.
    keys = []
    key_events = by["key"]
    for n, key in enumerate(key_events):
        t = key["t"]
        until = key_events[n + 1]["t"] if n + 1 < len(key_events) else t + 200_000
        until = min(until, t + 200_000)
        i = bisect_left(read_times, t)
        answer = []
        while i < len(reads) and reads[i]["t"] < until:
            # the burst ends where the program was quiet for 30 ms
            if answer and reads[i]["from"] - answer[-1]["t"] > 30_000:
                break
            answer.append(reads[i])
            i += 1
        if not answer:
            continue
        first, last = answer[0], answer[-1]
        shown = first_frame_after(first["t"])
        settled = first_frame_after(last["t"])
        if not shown or not settled:
            continue
        frames_between = bisect_left(frame_starts, settled["t"]) - bisect_left(frame_starts, shown["t"])
        keys.append(
            {
                "key": key["key"],
                "shell": first["t"] - t,
                "wait": shown["t"] - first["t"],
                "draw": shown["draw_end"] - shown["t"],
                "submit": shown["presented"] - shown["draw_end"],
                "latency": shown["presented"] - t,
                "settled": settled["presented"] - t,
                "reads": len(answer),
                "extra_frames": frames_between,
            }
        )

    # Glides of the cursor: from the frame where its cell changed until it rests or heads elsewhere.
    glides = []
    by_pane = defaultdict(list)
    for p in paints:
        by_pane[p["pane"]].append(p)
    for ps in by_pane.values():
        current = None
        for before, p in zip([None] + ps, ps):
            cell = (p.get("col"), p.get("row"))
            moved = before is not None and cell != (before.get("col"), before.get("row"))
            if current is not None and (moved or not p.get("moving")):
                if not moved:
                    current.append(p)
                glides.append(current)
                current = None
            if p.get("moving") and current is None:
                current = [p]
            elif current is not None:
                current.append(p)
    frames_per_glide = [len(g) for g in glides]
    gaps = []
    for g in glides:
        for a, b in zip(g, g[1:]):
            gaps.append(b["from"] - a["from"])

    # Frame signals that drew nothing: a frame the compositor offered and lumen let go by.
    signals = by["signal"]
    skips = Counter(s["reason"] for s in by["skip"])

    # the first frames load the fonts: that is startup, not lag
    since = key_events[0]["t"] if key_events else 0
    draw = [f["draw_end"] - f["t"] for f in frames if f["t"] >= since]
    prepaint = [p["t"] - p["from"] for p in paints if p["t"] >= since]
    stalls = defaultdict(lambda: [0, 0, 0])
    for p in by["poll"]:
        d = p["end"] - p["t"]
        if d >= 2_000:
            s = stalls[p["at"]]
            s[0] += 1
            s[1] += d
            s[2] = max(s[2], d)
    duration = events[-1]["t"] - events[0]["t"] if events else 0
    return {
        "duration": duration,
        "refresh": refresh,
        "frames": len(frames),
        "signals": len(signals),
        "skips": skips,
        "keys": keys,
        "glides": len(glides),
        "frames_per_glide": frames_per_glide,
        "glide_gaps": gaps,
        "draw": draw,
        "prepaint": prepaint,
        "stalls": stalls,
        "lost": sum(e.get("entries", 0) for e in by["lost"]),
        "drive": bool(events and events[0].get("drive")),
    }


def row(label, values, unit="ms", scale=ms):
    if not values:
        return f"  {label:<34} -"
    p = lambda q: scale(pct(values, q))
    return f"  {label:<34} {p(50):7.1f} {p(95):7.1f} {p(99):7.1f} {scale(max(values)):7.1f} {unit}"


def report(path, a):
    print(f"\n{path}")
    hz = 1e6 / a["refresh"] if a["refresh"] else None
    print(
        f"  {a['duration'] / 1e6:.1f} s, {len(a['keys'])} keys answered, {a['frames']} frames"
        + (f", display {hz:.0f} Hz ({a['refresh'] / 1000:.2f} ms a frame)" if hz else "")
        + (", scripted typing" if a["drive"] else "")
    )
    if a["lost"]:
        print(f"  ! {a['lost']} journal entries lost: the record has gaps")

    print(f"\n  {'from key to frame':<34} {'p50':>7} {'p95':>7} {'p99':>7} {'max':>7}")
    keys = a["keys"]
    print(row("key -> answer on screen", [k["latency"] for k in keys]))
    print(row("key -> last of the answer shown", [k["settled"] for k in keys]))
    print(row("  shell answers", [k["shell"] for k in keys]))
    print(row("  answer waits for a frame", [k["wait"] for k in keys]))
    print(row("  frame is drawn", [k["draw"] for k in keys]))
    print(row("  frame is handed over", [k["submit"] for k in keys]))
    kinds = Counter(k["key"] for k in keys)
    for kind, n in kinds.most_common(6):
        if n >= 3:
            print(row(f"  {kind} ({n})", [k["latency"] for k in keys if k["key"] == kind]))
    multi = [k for k in keys if k["reads"] > 1]
    if keys:
        shown_twice = [k for k in keys if k["extra_frames"] > 0]
        print(
            f"  the shell answered in more than one write to {len(multi)} of {len(keys)} keys;"
            f" {len(shown_twice)} answers took more than one frame to show"
        )

    print(f"\n  {'cursor glides (' + str(GLIDE_MS) + ' ms each)':<34} {'p50':>7} {'p95':>7} {'p99':>7} {'max':>7}")
    smooth = f" (smooth: ~{GLIDE_MS * 1000 / a['refresh']:.0f})" if a["refresh"] else ""
    print(row(f"  frames per glide{smooth}", a["frames_per_glide"], unit="", scale=lambda v: v))
    print(row("  time between glide frames", a["glide_gaps"]))
    if a["glide_gaps"] and a["refresh"]:
        late = sum(1 for g in a["glide_gaps"] if g > 1.5 * a["refresh"])
        print(f"  {late} of {len(a['glide_gaps'])} glide frames came a refresh or more late")
        typical = pct(a["glide_gaps"], 50)
        if typical > 1.5 * a["refresh"]:
            print(
                f"  ! frames came about {1e6 / typical:.0f} a second, slower than the display. GPUI holds"
                " back a window without the focus, and one in the power-saver profile unless lumen opts"
                " out (WindowOptions in src/main.rs); a busy main thread shows below"
            )

    print(f"\n  {'frames':<34} {'p50':>7} {'p95':>7} {'p99':>7} {'max':>7}")
    print(row("draw (all of GPUI's frame)", a["draw"]))
    print(row("prepaint of one pane's grid", a["prepaint"]))
    if a["refresh"]:
        over = sum(1 for d in a["draw"] if d > a["refresh"])
        print(f"  {over} of {len(a['draw'])} frames took longer to draw than a refresh")
    unused = a["signals"] - a["frames"]
    print(f"  {a['signals']} frame signals, {max(unused, 0)} of them drew nothing")
    for reason, n in a["skips"].most_common():
        print(f"    skipped: {reason} x{n}")

    if a["stalls"]:
        print("\n  main thread busy 2 ms or more at once")
        worst = sorted(a["stalls"].items(), key=lambda kv: -kv[1][1])[:8]
        for at, (n, total, longest) in worst:
            print(f"    {n:4d}x  {total / 1000:7.1f} ms in all, longest {longest / 1000:6.1f} ms  {at}")


def compare(paths, results):
    print(f"\n{'':<36}" + "".join(f"{p[-28:]:>30}" for p in paths))

    def line(label, values_of, q=50, unit="ms", scale=ms):
        cells = []
        for a in results:
            v = values_of(a)
            cells.append(f"{scale(pct(v, q)):27.1f} {unit:<2}" if v else f"{'-':>30}")
        print(f"  {label:<34}" + "".join(cells))

    line("key -> screen p50", lambda a: [k["latency"] for k in a["keys"]])
    line("key -> screen p95", lambda a: [k["latency"] for k in a["keys"]], 95)
    line("key -> settled p50", lambda a: [k["settled"] for k in a["keys"]])
    line("answer waits for a frame p50", lambda a: [k["wait"] for k in a["keys"]])
    line("frames per glide p50", lambda a: a["frames_per_glide"], unit="", scale=lambda v: v)
    line("time between glide frames p50", lambda a: a["glide_gaps"])
    line("time between glide frames p95", lambda a: a["glide_gaps"], 95)
    line("draw p95", lambda a: a["draw"], 95)


def main():
    paths = sys.argv[1:]
    if not paths or any(p in ("-h", "--help") for p in paths):
        print((__doc__ or "").strip())
        return 0 if paths else 2
    results = [analyze(load(p)) for p in paths]
    for path, a in zip(paths, results):
        report(path, a)
    if len(paths) > 1:
        compare(paths, results)
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
