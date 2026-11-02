#!/usr/bin/env python3
"""Checks the app translations.

- every L("…") key in the Swift code exists in every language;
- no language has a key unknown to the code (dead key);
- placeholders (%1$@, %2$@…) are the same in every language.

Non-zero exit code on any mismatch: called by scripts/build-app.sh.
"""
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ROOT / "app" / "Sources" / "Kubeport"
RESOURCES = ROOT / "app" / "Resources"
KEY = re.compile(r'\bL\("([A-Za-z0-9_.]+)"')
PLACEHOLDER = re.compile(r"%\d+\$@")


def used_keys():
    keys = set()
    for path in SOURCES.rglob("*.swift"):
        if "Generated" in path.parts:
            continue
        keys.update(KEY.findall(path.read_text(encoding="utf-8")))
    return keys


def strings(lproj):
    path = lproj / "Localizable.strings"
    out = subprocess.run(
        ["plutil", "-convert", "json", "-o", "-", str(path)],
        capture_output=True, text=True, check=True,
    )
    return json.loads(out.stdout)


def main():
    used = used_keys()
    languages = {p.stem: strings(p) for p in sorted(RESOURCES.glob("*.lproj"))}
    errors = []
    for lang, table in languages.items():
        for key in sorted(used - table.keys()):
            errors.append(f"{lang}: missing key {key}")
        for key in sorted(table.keys() - used):
            errors.append(f"{lang}: unused key {key}")
    reference = languages.get("en", {})
    for lang, table in languages.items():
        for key in reference.keys() & table.keys():
            if sorted(PLACEHOLDER.findall(reference[key])) != sorted(PLACEHOLDER.findall(table[key])):
                errors.append(f"{lang}: placeholders differ from en for {key}")
    for error in errors:
        print(error, file=sys.stderr)
    print(f"{len(used)} keys, languages: {', '.join(languages)} — {'OK' if not errors else f'{len(errors)} mismatch(es)'}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
