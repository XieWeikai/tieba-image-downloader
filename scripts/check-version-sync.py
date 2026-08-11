#!/usr/bin/env python3
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def json_version(path: str, *keys: object) -> str:
    with (ROOT / path).open(encoding="utf-8") as handle:
        value = json.load(handle)
    for key in keys:
        value = value[key]
    return value


def required_match(pattern: str, text: str, source: str) -> str:
    match = re.search(pattern, text, re.MULTILINE)
    if match is None:
        raise SystemExit(f"could not read version from {source}")
    return match.group(1)


cargo_manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
cargo_version = required_match(
    r'^\[package\]\s*\n(?:.*\n)*?version\s*=\s*"([^"]+)"',
    cargo_manifest,
    "Cargo.toml",
)

cargo_lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
lock_version = required_match(
    r'^\[\[package\]\]\s*\nname\s*=\s*"tieba-image-downloader"\s*\nversion\s*=\s*"([^"]+)"',
    cargo_lock,
    "Cargo.lock",
)

run_script = (
    ROOT
    / "plugins/tieba-image-downloader/skills/download-tieba-images/scripts/run.sh"
).read_text(encoding="utf-8")
run_version = required_match(
    r'^PLUGIN_VERSION="([^"]+)"$', run_script, "run.sh"
)

versions = {
    "Cargo.toml": cargo_version,
    "Cargo.lock": lock_version,
    ".claude-plugin/marketplace.json": json_version(
        ".claude-plugin/marketplace.json", "plugins", 0, "version"
    ),
    "Codex plugin manifest": json_version(
        "plugins/tieba-image-downloader/.codex-plugin/plugin.json", "version"
    ),
    "Claude plugin manifest": json_version(
        "plugins/tieba-image-downloader/.claude-plugin/plugin.json", "version"
    ),
    "run.sh": run_version,
}

errors = [
    f"{source}: expected {cargo_version}, found {version}"
    for source, version in versions.items()
    if version != cargo_version
]

if len(sys.argv) > 2:
    raise SystemExit("usage: check-version-sync.sh [vX.Y.Z]")
if len(sys.argv) == 2:
    tag_version = sys.argv[1].removeprefix("v")
    if tag_version != cargo_version:
        errors.append(
            f"release tag: expected v{cargo_version}, found {sys.argv[1]}"
        )

if errors:
    print("Version synchronization failed:", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"All version fields are synchronized at {cargo_version}.")
