"""Benchmark the Python and Rust Lugh runtimes on the same programs.

Measures mean wall time and peak resident memory (each in a fresh process).
Usage: python bench/gen.py && python bench/run.py
"""
import os, statistics, subprocess, sys, time

RUNTIMES = {
    "python": [sys.executable, "lugh.py"],
    "rust": ["./core/target/release/lugh"],
}
CASES = [("startup", "bench/hello.lugh", 20), ("heavy", "bench/heavy.lugh", 3)]

def clean():
    for f in ("lugh_memory.json", "lugh_stats.json", "lugh_audit.jsonl"):
        if os.path.exists(f):
            os.remove(f)

PEAKRSS = "bench/peakrss"


def build_probe():
    if not os.path.exists(PEAKRSS):
        subprocess.run(["cc", "-O2", "-o", PEAKRSS, "bench/peakrss.c"], check=True)


def measure(cmd):
    """Run cmd once via the C probe; return (seconds, peak RSS in MB of that program alone)."""
    t = time.perf_counter()
    out = subprocess.run([PEAKRSS, *cmd], capture_output=True, text=True, check=True).stdout
    return time.perf_counter() - t, int(out.strip()) / 1024


build_probe()
results = {}
for case, prog, n in CASES:
    for name, base in RUNTIMES.items():
        cmd = base + [prog, "--yes"]
        clean(); measure(cmd)  # warm-up (file cache)
        times, mems = [], []
        for _ in range(n):
            clean()
            t, m = measure(cmd)
            times.append(t); mems.append(m)
        results[(case, name)] = (statistics.mean(times), max(mems))
        print(f"{case:8} {name:7} {statistics.mean(times)*1000:10.1f} ms   peak memory {max(mems):6.1f} MB", flush=True)
clean()
print()
for case, _, _ in CASES:
    (pt, pm), (rt, rm) = results[(case, "python")], results[(case, "rust")]
    print(f"{case}: rust {pt/rt:.1f}x faster, {pm/rm:.1f}x less memory")
size = os.path.getsize("./core/target/release/lugh") / 1024 / 1024
print(f"rust binary: {size:.2f} MB, no runtime dependencies beyond libc")
