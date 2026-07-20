#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NPM_REPOSITORY_URL = "https://github.com/ip2a/openpage"


def read_cargo_version() -> str:
    cargo_toml = (ROOT / "rust" / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, flags=re.MULTILINE)
    if not match:
        raise RuntimeError("Version not found in rust/Cargo.toml")
    return match.group(1)


def update_json_version(path: Path, version: str) -> bool:
    data = json.loads(path.read_text(encoding="utf-8"))
    changed = False

    if data.get("version") != version:
        data["version"] = version
        changed = True

    repository = data.get("repository")
    if not isinstance(repository, dict):
        repository = {"type": "git"}
    if repository.get("type") != "git":
        repository["type"] = "git"
        changed = True
    if repository.get("url") != NPM_REPOSITORY_URL:
        repository["url"] = NPM_REPOSITORY_URL
        changed = True
    data["repository"] = repository

    if "optionalDependencies" in data:
        for name, current in list(data["optionalDependencies"].items()):
            if current != version:
                data["optionalDependencies"][name] = version
                changed = True

    if changed:
        path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return changed


def update_pyproject_version(path: Path, version: str) -> bool:
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    in_project = False
    changed = False
    replaced = False

    for idx, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[project]":
            in_project = True
            continue
        if stripped.startswith("[") and stripped != "[project]":
            in_project = False
        if in_project and stripped.startswith("version ="):
            new_line = f'version = "{version}"\n'
            if lines[idx] != new_line:
                lines[idx] = new_line
                changed = True
            replaced = True

    if not replaced:
        raise RuntimeError(f"Missing [project].version in file: {path}")

    if changed:
        path.write_text("".join(lines), encoding="utf-8")
    return changed


def load_platforms() -> list[dict[str, str]]:
    platforms_toml = (ROOT / "platforms.toml").read_text(encoding="utf-8")
    return tomllib.loads(platforms_toml)["platforms"]


def main() -> None:
    parser = argparse.ArgumentParser(description="Sync release versions from rust/Cargo.toml")
    parser.add_argument("--check", action="store_true", help="fail if any file is out of sync")
    args = parser.parse_args()

    version = read_cargo_version()
    print(f"[info] Unified version: {version}")

    changed_paths: list[Path] = []

    python_pyproject = ROOT / "python" / "pyproject.toml"
    if update_pyproject_version(python_pyproject, version):
        changed_paths.append(python_pyproject)

    npm_packages_dir = ROOT / "npm" / "packages"
    if npm_packages_dir.exists():
        for json_file in sorted(npm_packages_dir.rglob("package.json")):
            if update_json_version(json_file, version):
                changed_paths.append(json_file)

    if args.check and changed_paths:
        print("[error] Release metadata is out of sync:")
        for path in changed_paths:
            print(f"  - {path.relative_to(ROOT)}")
        raise SystemExit(1)

    for path in changed_paths:
        print(f"[ok] Synced version: {path.relative_to(ROOT)}")

    if not changed_paths:
        print("[ok] Release metadata already in sync")

    platform_names = [platform["npm_package"] for platform in load_platforms()]
    print(f"[info] Loaded platform map for {len(platform_names)} npm platform packages")


if __name__ == "__main__":
    main()
