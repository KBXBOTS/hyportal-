"""Pack HyPortal's PNG icons into a Windows .ico.

The PNGs are produced from the brand logo by `tools/make_icons.ps1`, which
resizes with high-quality bicubic sampling. Sizes at or below 64px are cropped
to the portal arch alone — the "HyPortal" wordmark in the full lockup turns
illegible below about 64px, so shrinking the whole image would waste the icon.

Windows Vista and later read PNG data embedded directly in an .ico, so this is
just a container format: a header, one directory entry per size, then the PNG
bytes back to back. No image library required.
"""

import os
import struct

ICONS = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src-tauri", "icons"
)

# Sizes to embed, smallest first, mapped to the PNG that holds each one.
MEMBERS = [
    (16, "16x16.png"),
    (32, "32x32.png"),
    (48, "48x48.png"),
    (64, "64x64.png"),
    (128, "128x128.png"),
    (256, "128x128@2x.png"),
]


def ico_bytes(members):
    pngs = []
    for size, name in members:
        path = os.path.join(ICONS, name)
        if not os.path.isfile(path):
            raise SystemExit(f"missing {name} — run tools/make_icons.ps1 first")
        with open(path, "rb") as fh:
            pngs.append((size, fh.read()))

    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = 6 + 16 * len(pngs)
    entries, blobs = b"", b""
    for size, data in pngs:
        # A dimension of 0 in the directory means 256px.
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    return header + entries + blobs


def main():
    out = os.path.join(ICONS, "icon.ico")
    data = ico_bytes(MEMBERS)
    with open(out, "wb") as fh:
        fh.write(data)
    sizes = ", ".join(str(s) for s, _ in MEMBERS)
    print(f"wrote icon.ico ({len(data):,} bytes) containing {sizes}")


if __name__ == "__main__":
    main()
