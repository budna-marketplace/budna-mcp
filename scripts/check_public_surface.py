#!/usr/bin/env python3
"""Fail when the repository contains non-portable or non-public material."""

from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from pathlib import Path


SELF = Path("scripts/check_public_surface.py")
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
TEXT_RULES = (
    ("parent-relative path", re.compile(r"\.\.[/\\]")),
    ("local macOS home path", re.compile(r"/Users/[^/\s]+/")),
    ("local Linux home path", re.compile(r"/home/[^/\s]+/")),
    ("local Windows home path", re.compile(r"[A-Za-z]:\\Users\\[^\\\s]+\\")),
    ("local file URL", re.compile(r"\bfile://", re.IGNORECASE)),
    ("SSH repository URL", re.compile(r"\b(?:ssh://|git@[\w.-]+:)", re.IGNORECASE)),
    (
        "public environment selector",
        re.compile(
            r"(?:--env(?:ironment)?\b|\bBUDNA_[A-Z_]*ENV(?:IRONMENT)?\b)"
        ),
    ),
    (
        "non-public service hostname",
        re.compile(
            r"https?://(?!api\.budna\.se(?:/|$))[A-Za-z0-9.-]+\.budna\.se(?:/|$)",
            re.IGNORECASE,
        ),
    ),
    (
        "non-public source provenance",
        re.compile(
            r"\b(?:sibling|companion|private|internal)\s+"
            r"(?:source\s+)?(?:repository|repo|codebase|workspace)\b",
            re.IGNORECASE,
        ),
    ),
)
SECRET_RULES = (
    ("private key", re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")),
    ("AWS access key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("GitHub token", re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{30,}\b")),
    ("API key", re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b")),
    ("Slack token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{20,}\b")),
    (
        "JWT",
        re.compile(
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\."
            r"[A-Za-z0-9_-]{10,}\b"
        ),
    ),
)
FORBIDDEN_IDENTIFIER_HASHES = frozenset(
    {
        "e748a026b6fba5c37082acf35c569765fb1e8bfeeb1da3b47b799ece282b370e",
        "af309e70a8bd0e883d397237f6b9ecdb67a3a09d199b81d7d4f764371abdbc35",
        "a83bbba159c18c1a2b42e3ce78f7e359c25f4df178ae900049dbdea9e787957f",
        "d5e1efdfba7aeedab6787849eb858665c240587699efa2e9991473f06d793266",
        "605ecc6c72b2a8dd124c1aadab82a10d6a9135f6fb4ee02bc9fffcd2bce973f0",
        "3993e43717729d68d6ddeab7576c9b10e876e37c7f7d7a7bb2f2feee0e7d5b35",
        "13534a1dac6ecf141a74fba6ea7a7507da1a385649aa333a78b77fdac5b762ea",
        "29abdbc9220e4beef0fce228ddf9f2c5ed0a5fc13f21549ece6b47fd833c7d06",
        "a375fb2168923ce5439f0905ae9afaa134b190ff790172a1a09a5f042bfe0f1e",
        "8e43d60f9c9ff27bec60c1f2e36d64edd96169c97a248d9b68e7c1821aa3e033",
        "a622ab76713f733cb61965e409b8ee3962d779f3fe315ad5f5fef4873651146c",
        "7f17766e365e559f56302a355b4a09a35fbf0b38d8e924acf7883da80560640c",
        "91f02cbc681b0e91127ec31aac35a62abed8a8bfcad1c8d96843b292bcef357a",
        "d3c68ef685e77e2b29d44148aa1a6e65445119dc68bef7271a32aff755c99a38",
    }
)


def git(root: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", *args],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def repository_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return Path(result.stdout.strip()).resolve()


def decode_text(data: bytes) -> str | None:
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def inspect_text(label: str, text: str, violations: set[str]) -> None:
    for line_number, line in enumerate(text.splitlines(), start=1):
        for rule_name, pattern in (*TEXT_RULES, *SECRET_RULES):
            if pattern.search(line):
                violations.add(f"{label}:{line_number}: {rule_name}")

        for identifier in IDENTIFIER.findall(line):
            digest = hashlib.sha256(identifier.encode()).hexdigest()
            if digest in FORBIDDEN_IDENTIFIER_HASHES:
                violations.add(
                    f"{label}:{line_number}: non-public implementation identifier"
                )


def inspect_path(label: str, path: str, mode: str, violations: set[str]) -> None:
    path_object = Path(path)
    if mode == "120000":
        violations.add(f"{label}: symlinks are not allowed")
    if path == ".gitmodules":
        violations.add(f"{label}: Git submodules are not allowed")
    if path_object.name.startswith(".env") and path_object.name != ".env.example":
        violations.add(f"{label}: environment files must not be tracked")


def inspect_worktree(root: Path, violations: set[str]) -> None:
    listed = git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z")
    for raw_path in listed.split(b"\0"):
        if not raw_path:
            continue
        relative = Path(raw_path.decode("utf-8", errors="surrogateescape"))
        label = relative.as_posix()
        full_path = root / relative
        mode = "120000" if full_path.is_symlink() else "100644"
        inspect_path(label, label, mode, violations)
        if relative == SELF or full_path.is_symlink() or not full_path.is_file():
            continue
        text = decode_text(full_path.read_bytes())
        if text is not None:
            inspect_text(label, text, violations)


def inspect_history(root: Path, violations: set[str]) -> None:
    commits = git(root, "rev-list", "--all").decode().splitlines()
    seen_blobs: set[str] = set()

    for commit in commits:
        tree = git(root, "ls-tree", "-rz", "--full-tree", commit)
        for record in tree.split(b"\0"):
            if not record:
                continue
            metadata, raw_path = record.split(b"\t", maxsplit=1)
            mode, object_type, object_id = metadata.decode().split()
            if object_type != "blob":
                continue
            path = raw_path.decode("utf-8", errors="surrogateescape")
            label = f"{commit[:12]}:{path}"
            inspect_path(label, path, mode, violations)
            if path == SELF.as_posix() or mode == "120000" or object_id in seen_blobs:
                continue
            seen_blobs.add(object_id)
            text = decode_text(git(root, "cat-file", "blob", object_id))
            if text is not None:
                inspect_text(label, text, violations)


def main() -> int:
    unexpected = set(sys.argv[1:]) - {"--working-tree-only"}
    if unexpected:
        print(f"unsupported arguments: {', '.join(sorted(unexpected))}", file=sys.stderr)
        return 2

    root = repository_root()
    violations: set[str] = set()
    inspect_worktree(root, violations)
    if "--working-tree-only" not in sys.argv[1:]:
        inspect_history(root, violations)

    if violations:
        print("Public-surface check failed:", file=sys.stderr)
        for violation in sorted(violations):
            print(f"- {violation}", file=sys.stderr)
        return 1

    print("Public-surface check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
