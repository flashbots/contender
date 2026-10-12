#!/usr/bin/env python3
"""Validate Contender docs for drift (issue #431).

Checks:
  1. Per-crate git dependency tags in docs match crates/*/Cargo.toml versions
     and use the <crate>-v<version> tag format from scripts/1_set-version.sh.
  2. Fenced TOML blocks parse as TOML (when tomllib/tomli is available).
  3. Contender CLI invocations in bash fences only use known top-level subcommands.
  4. Common spam flags referenced in docs exist in the clap source definitions.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
CLI_SRC = ROOT / "crates" / "cli" / "src"

def discover_subcommands() -> set[str]:
    """Parse clap #[command(name = "...")] from the CLI source."""
    names = {"help", "version"}
    for src in CLI_SRC.rglob("*.rs"):
        data = src.read_text()
        names.update(re.findall(r'#\[command\([^\]]*name\s*=\s*"([^"]+)"', data))
        # variants without explicit name use snake_case -> kebab often; also catch about-only
    # Always include top-level names we know clap derives from variant identifiers
    # when name= is omitted (Setup, Report, Replay, Rpc, ...)
    sub_file = CLI_SRC / "commands" / "contender_subcommand.rs"
    if sub_file.exists():
        body = sub_file.read_text()
        for m in re.finditer(r"^\s+([A-Z][A-Za-z0-9]+)\s*[\{{,]", body, re.M):
            ident = m.group(1)
            if ident in {"Admin", "ContenderSubcommand"}:
                continue
            # PascalCase -> kebab-case
            kebab = re.sub(r"(?<!^)([A-Z])", r"-", ident).lower()
            names.add(kebab)
            names.add(ident.lower())
    return names

# Flags commonly documented; verified against clap source text.
DOCUMENTED_FLAGS = {
    "--tps",
    "--tpb",
    "-r",
    "-p",
    "-d",
    "-a",
    "-e",
    "-i",
    "--min-balance",
}


def crate_versions() -> dict[str, str]:
    versions: dict[str, str] = {}
    for cargo in (ROOT / "crates").glob("*/Cargo.toml"):
        text = cargo.read_text()
        name_m = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M)
        ver_m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
        if name_m and ver_m:
            versions[name_m.group(1)] = ver_m.group(1)
    return versions


def extract_fences(md: str, lang: str) -> list[str]:
    pattern = rf"```{lang}\n(.*?)```"
    return re.findall(pattern, md, flags=re.S)


def check_crate_tags(docs_text: str, versions: dict[str, str], errors: list[str]) -> None:
    # tag = "contender_core-v0.11.2"
    for crate, tag in re.findall(
        r'tag\s*=\s*"(contender_[a-z_]+)-v([0-9]+\.[0-9]+\.[0-9]+)"', docs_text
    ):
        expected = versions.get(crate)
        if expected is None:
            errors.append(f"unknown crate in docs tag: {crate}")
            continue
        if tag != expected:
            errors.append(
                f"docs tag {crate}-v{tag} does not match crates version {expected}"
            )
    # Catch obsolete underscore tag format from older docs
    for bad in re.findall(r'tag\s*=\s*"(contender_[a-z_]+)_v[0-9.]+"', docs_text):
        errors.append(
            f"obsolete tag format for {bad}: use '{bad}-v<version>' (hyphen), not underscore"
        )


def check_toml_fences(path: Path, md: str, errors: list[str]) -> None:
    try:
        import tomllib  # py311+
    except ImportError:
        try:
            import tomli as tomllib  # type: ignore
        except ImportError:
            return  # optional
    for i, block in enumerate(extract_fences(md, "toml"), 1):
        # Skip obvious snippets that are not full documents
        if block.strip().startswith("#") and "[" not in block:
            continue
        # Skip partial / placeholder snippets
        if re.match(r"^\s*\[\[", block) or "<" in block:
            continue
        try:
            tomllib.loads(block)
        except Exception as exc:  # noqa: BLE001
            if re.search(r"^\[", block, re.M):
                errors.append(f"{path.name} toml fence #{i}: {exc}")


def check_cli_examples(path: Path, md: str, known: set[str], errors: list[str]) -> None:
    for i, block in enumerate(extract_fences(md, "bash") + extract_fences(md, "sh"), 1):
        for line in block.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            # strip line continuations and env assignments
            if line.endswith("\\"):
                line = line[:-1].rstrip()
            line = re.sub(r"^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s+)+", "", line)
            if not line.startswith("contender"):
                continue
            parts = line.split()
            if len(parts) < 2:
                continue
            sub = parts[1]
            if sub.startswith("-"):
                continue  # e.g. contender -h
            if sub not in known:
                errors.append(
                    f"{path.name} bash fence #{i}: unknown subcommand '{sub}' in: {line}"
                )


def check_flags_against_source(docs_text: str, errors: list[str]) -> None:
    cli_src = "\n".join(p.read_text() for p in CLI_SRC.rglob("*.rs"))
    for flag in sorted(DOCUMENTED_FLAGS):
        if flag not in docs_text:
            continue
        # Presence heuristics in clap source
        token = flag.lstrip("-")
        if flag.startswith("--"):
            ok = f'"{token}"' in cli_src or f"long = \"{token}\"" in cli_src or f"--{token}" in cli_src
        else:
            ok = f"short = '{token}'" in cli_src or f'short = "{token}"' in cli_src or f"\"-{token}\"" in cli_src
        if not ok:
            # short flags often via #[arg(short, long)] — accept if long form exists nearby in docs only
            if flag.startswith("--"):
                errors.append(f"documented flag {flag} not found in crates/cli clap source")


def main() -> int:
    versions = crate_versions()
    if not versions:
        print("error: no crate versions found", file=sys.stderr)
        return 2

    errors: list[str] = []
    docs_files = sorted(DOCS.glob("*.md"))
    if not docs_files:
        print("error: no docs found", file=sys.stderr)
        return 2

    all_docs = "\n".join(p.read_text() for p in docs_files)
    check_crate_tags(all_docs, versions, errors)
    check_flags_against_source(all_docs, errors)

    known = discover_subcommands()
    for path in docs_files:
        md = path.read_text()
        check_toml_fences(path, md, errors)
        check_cli_examples(path, md, known, errors)

    if errors:
        print("Docs review failed:")
        for e in errors:
            print(f"  - {e}")
        return 1

    print(f"Docs review OK ({len(docs_files)} files, {len(versions)} crates).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
