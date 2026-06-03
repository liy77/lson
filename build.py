#!/usr/bin/env python3
"""
LSON cross-platform build script.

Builds release binaries for Linux x86_64, Windows x86_64, and macOS ARM64.
Automatically installs Rust targets via rustup.

Usage:
  python build.py              # build all targets
  python build.py linux        # build only the linux target
  python build.py windows macos-arm64
"""

import subprocess
import sys
import shutil
import os
from pathlib import Path

# ── Target definitions ────────────────────────────────────────────────────────

TARGETS: list[dict] = [
    {
        "name": "linux",
        "triple": "x86_64-unknown-linux-gnu",
        "bin": "lson",
        "output": "lson-linux-x86_64",
        "strip": True,
    },
    {
        "name": "windows",
        "triple": "x86_64-pc-windows-msvc",
        "bin": "lson.exe",
        "output": "lson-windows-x86_64.exe",
        "strip": False,
    },
    {
        "name": "macos-arm64",
        "triple": "aarch64-apple-darwin",
        "bin": "lson",
        "output": "lson-macos-arm64",
        "strip": True,
    },
]

DIST = Path("dist")

# ── Helpers ───────────────────────────────────────────────────────────────────

CYAN  = "\033[1;36m"
GREEN = "\033[1;32m"
RED   = "\033[1;31m"
DIM   = "\033[2m"
RESET = "\033[0m"


def step(msg: str) -> None:
    print(f"\n{CYAN}==> {msg}{RESET}")


def ok(msg: str) -> None:
    print(f"  {GREEN}✓{RESET} {msg}")


def run(cmd: list[str], *, check: bool = True) -> subprocess.CompletedProcess:
    print(f"  {DIM}$ {' '.join(str(c) for c in cmd)}{RESET}")
    result = subprocess.run(cmd)
    if check and result.returncode != 0:
        print(f"\n{RED}Command failed (exit {result.returncode}){RESET}", file=sys.stderr)
        sys.exit(result.returncode)
    return result


def install_target(triple: str) -> None:
    run(["rustup", "target", "add", triple])


def build_target(t: dict) -> None:
    step(f"Building  {t['output']}")

    run(["cargo", "build", "--release", "--target", t["triple"]])

    src = Path("target") / t["triple"] / "release" / t["bin"]
    dst = DIST / t["output"]

    shutil.copy2(src, dst)

    if t["strip"]:
        strip_bin = shutil.which("strip")
        if strip_bin:
            run([strip_bin, str(dst)], check=False)
        else:
            print(f"  {DIM}(strip not found — skipping){RESET}")

    size_kb = dst.stat().st_size // 1024
    ok(f"{dst}  ({size_kb} KB)")


# ── Entry point ───────────────────────────────────────────────────────────────

def main() -> None:
    # Determine which targets to build
    requested = sys.argv[1:]
    if requested:
        targets = [t for t in TARGETS if t["name"] in requested]
        unknown = set(requested) - {t["name"] for t in TARGETS}
        if unknown:
            names = ", ".join(t["name"] for t in TARGETS)
            print(f"{RED}Unknown target(s): {', '.join(unknown)}{RESET}", file=sys.stderr)
            print(f"Available: {names}", file=sys.stderr)
            sys.exit(1)
    else:
        targets = TARGETS

    step("Installing Rust targets")
    for t in targets:
        install_target(t["triple"])

    DIST.mkdir(exist_ok=True)

    for t in targets:
        build_target(t)

    step("Done")
    print()
    for t in targets:
        f = DIST / t["output"]
        if f.exists():
            print(f"  {f}  ({f.stat().st_size // 1024} KB)")
    print()


if __name__ == "__main__":
    main()
