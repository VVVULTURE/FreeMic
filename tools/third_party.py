"""Writes THIRD-PARTY-NOTICES.md from `cargo metadata` (run from the repo root)."""
import json, subprocess, sys
pkgs = {}
for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]:
    m = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--filter-platform", target]))
    nodes = {n["id"]: n for n in m["resolve"]["nodes"]}
    seen, st = set(), [m["resolve"]["root"]]
    while st:
        x = st.pop()
        if x in seen: continue
        seen.add(x)
        for d in nodes[x]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                st.append(d["pkg"])
    byid = {p["id"]: p for p in m["packages"]}
    for i in seen:
        p = byid[i]
        if p["name"] == "freemic": continue
        key = (p["name"], p["version"])
        pkgs.setdefault(key, {"license": p.get("license") or "see repository", "repo": p.get("repository") or "", "targets": set()})["targets"].add("Windows" if "windows" in target else "Linux")
out = ["# Third-party notices", "",
       "FreeMic is MIT licensed. It is built from the open-source Rust crates listed below,",
       "each under its own license (identifier shown; full texts are in each crate's source).", "",
       "* **Symphonia** (MPL-2.0) is used unmodified; its source code is available at https://github.com/pdeljanov/Symphonia.",
       "* egui's bundled fonts: Ubuntu font (Ubuntu Font Licence 1.0), Hack and emoji fonts (OFL-1.1 / MIT / Apache-2.0).",
       "* **Inter** typeface 4.1 (SIL Open Font License 1.1), Copyright (c) 2016 The Inter Project Authors, https://github.com/rsms/inter. Embedded unmodified; the license text is in `assets/fonts/Inter-LICENSE.txt`.",
       "* **yt-dlp** (https://github.com/yt-dlp/yt-dlp) runs as a separate program. **Windows packages include** the official, unmodified `yt-dlp.exe`. yt-dlp itself is released under the Unlicense; its PyInstaller-built standalone executable also contains Python and other components, and as a whole is licensed under the **GNU GPL v3 or later**. Its license texts ship next to it (`yt-dlp-LICENSE.txt`, `yt-dlp-THIRD_PARTY_LICENSES.txt`) together with `yt-dlp-SOURCE.txt`, which says where its source code is. FreeMic only starts it as a separate program and passes it command-line arguments, so FreeMic itself remains MIT licensed. On Linux, yt-dlp is not included: FreeMic downloads the official `yt-dlp_linux` from yt-dlp's releases when you first use the YouTube page.",
       "* The YouTube integration ports the search and download logic of YT-Downloader (https://github.com/VVVULTURE/YT-Downloader) by the same author.",
       "* On Linux FreeMic links dynamically to the system's libpulse (LGPL-2.1+).",
       "* VB-CABLE is **not** included, redistributed or downloaded by FreeMic. It is a separate product of VB-Audio Software (donationware) that you install from vb-audio.com.", "",
       "| Crate | Version | License | Platforms | Source |", "|---|---|---|---|---|"]
for (n, v), d in sorted(pkgs.items()):
    out.append(f"| {n} | {v} | {d['license']} | {', '.join(sorted(d['targets']))} | {d['repo']} |")
open("THIRD-PARTY-NOTICES.md", "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
print(len(pkgs), "crates written")
