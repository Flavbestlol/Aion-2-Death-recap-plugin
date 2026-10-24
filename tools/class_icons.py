"""Fetches the class emblems the overlay shows into app/ui/img/class/.

They come from the A2 Tools DPS Meter's repository (public/assets), at the
same commit Flav-Toolkit's parser is pinned to, as Flav-Toolkit's own build
copied them. They are not committed here (game art is fetched, never kept in
the repository). Run once per clone, before `cargo build`; CI runs it too.

    python tools/class_icons.py
"""

import pathlib
import sys
import urllib.parse
import urllib.request

# Flav-Toolkit's UPSTREAM.toml at the core commit this repository pins.
METER = "e6d4f6bec042fd44fbb0a4c9c3bd3519f78f634f"
SOURCES = [
    f"https://raw.githubusercontent.com/taengu/A2Tools-DPS-Meter/{METER}/public/assets/",
    # Flavbest's backup fork of the meter, when the original is gone.
    f"https://raw.githubusercontent.com/Flavbestlol/A2Tools-DPS-Meter/{METER}/public/assets/",
]
# The meter names them in Korean; the overlay asks by the game's class code.
CLASSES = {
    "GL": "검성.png",
    "TE": "수호성.png",
    "AS": "살성.png",
    "RA": "궁성.png",
    "SO": "마도성.png",
    "EL": "정령성.png",
    "CL": "치유성.png",
    "CH": "호법성.png",
    "GT": "권성.png",
}


def fetch(name: str) -> bytes:
    last = None
    for base in SOURCES:
        try:
            with urllib.request.urlopen(base + urllib.parse.quote(name), timeout=30) as r:
                data = r.read()
            if data.startswith(b"\x89PNG"):
                return data
            last = f"not a PNG from {base}"
        except Exception as e:  # noqa: BLE001 - try the next source
            last = f"{base}: {e}"
    raise RuntimeError(f"{name}: {last}")


def main() -> int:
    out = pathlib.Path(__file__).resolve().parent.parent / "app" / "ui" / "img" / "class"
    out.mkdir(parents=True, exist_ok=True)
    for code, name in CLASSES.items():
        (out / f"{code}.png").write_bytes(fetch(name))
    print(f"{len(CLASSES)} class emblems in {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
