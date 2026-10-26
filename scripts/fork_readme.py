#!/usr/bin/env python3
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Generate the fork's milestone graphic from reviewed, source-controlled acceptance criteria.

These counts are not effort estimates or a percentage of the ultimate product.
"""
import argparse
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def progress(root):
    data = json.loads((root / "fork-progress.json").read_text())
    if data.get("schema") != 1 or len(data.get("areas", [])) != 3:
        raise ValueError("expected milestone schema 1 with three areas")
    for area in data["areas"]:
        if not area["milestones"]:
            raise ValueError("an area must have acceptance criteria")
        for item in area["milestones"]:
            if type(item.get("done")) is not bool:
                raise ValueError("each acceptance criterion needs a boolean status")
            if item["done"]:
                evidence = item.get("evidence", "")
                path = (root / evidence).resolve()
                if not evidence or not path.is_relative_to(root.resolve()) or not path.is_file():
                    raise ValueError("completed milestones need existing local evidence")
    return data["areas"]


def svg(areas):
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="960" height="310" viewBox="0 0 960 310" role="img" aria-label="Fork acceptance milestones">',
             '<!-- Copyright (C) 2026 The Suhina contributors; SPDX-License-Identifier: GPL-3.0-or-later -->',
             '<rect width="960" height="310" rx="20" fill="#111726"/>',
             '<g font-family="system-ui, sans-serif">',
             '<text x="32" y="40" fill="#f1f4ff" font-size="19" font-weight="700">A visible path from prototype to shared music workflow</text>']
    for i, area in enumerate(areas):
        done = sum(m["done"] for m in area["milestones"])
        total = len(area["milestones"])
        y = 81 + i * 67
        color = html.escape(area["color"], quote=True)
        parts += [f'<text x="32" y="{y}" fill="#dce3f5" font-size="16">{html.escape(area["name"])}</text>',
                  f'<text x="928" y="{y}" text-anchor="end" fill="{color}" font-size="15">{done} / {total} milestones implemented</text>',
                  f'<rect x="32" y="{y + 12}" width="896" height="9" rx="4.5" fill="#283147"/>',
                  f'<rect x="32" y="{y + 12}" width="{896 * done / total:.2f}" height="9" rx="4.5" fill="{color}"/>']
    parts += ['<text x="32" y="286" fill="#9caac2" font-size="13">Counts of the listed acceptance criteria. Not an effort estimate or a release-readiness score.</text>', '</g></svg>']
    return "\n".join(parts) + "\n"


def table(areas):
    lines = ["<!-- fork-progress:begin -->", "| Workflow | Implemented acceptance criteria | Next acceptance gate |", "|---|---:|---|"]
    for area in areas:
        done = sum(m["done"] for m in area["milestones"])
        pending = next((m["name"] for m in area["milestones"] if not m["done"]), "All listed criteria implemented")
        lines.append(f'| {area["name"]} | {done} / {len(area["milestones"])} | {pending} |')
    lines.append("<!-- fork-progress:end -->")
    return "\n".join(lines)


def generate(check=False):
    areas = progress(ROOT)
    graphic = ROOT / "assets/fork/progress.svg"
    readme = ROOT / "README.md"
    text = readme.read_text()
    start, end = "<!-- fork-progress:begin -->", "<!-- fork-progress:end -->"
    if text.count(start) != 1 or text.count(end) != 1:
        raise ValueError("README needs exactly one milestone region")
    before, rest = text.split(start)
    _, after = rest.split(end)
    outputs = {graphic: svg(areas), readme: before + table(areas) + after}
    for path, desired in outputs.items():
        if check:
            if not path.exists() or path.read_text() != desired:
                raise ValueError(f"stale milestone presentation: {path.relative_to(ROOT)}")
        else:
            path.write_text(desired)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    generate(parser.parse_args().check)
