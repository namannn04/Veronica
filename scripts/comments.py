from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from tree_sitter_language_pack import get_parser

if TYPE_CHECKING:
    from tree_sitter import Node

LANGUAGES = {
    ".rs": "rust",
    ".ts": "typescript",
    ".tsx": "tsx",
    ".js": "javascript",
    ".mjs": "javascript",
    ".cjs": "javascript",
    ".css": "css",
    ".sh": "bash",
    ".py": "python",
    ".yml": "yaml",
    ".yaml": "yaml",
    ".toml": "toml",
    ".html": "html",
}

EXCLUDED_PREFIXES = ("packaging/flatpak/shared-modules/",)

COMMENT_TYPES = {"comment", "line_comment", "block_comment"}

DIRECTIVE = re.compile(
    r"@ts-|eslint-|biome-ignore|shellcheck |noqa|type: ignore|yamllint |"
    r"swift-format|swiftlint|istanbul ignore"
)


def is_allowed(text: bytes, start: int) -> bool:
    if start == 0 and text.startswith(b"#!"):
        return True
    body = text.decode("utf-8", "replace")
    return body.startswith("/*!") or bool(DIRECTIVE.search(body))


def collect(node: Node, source: bytes, found: list) -> None:
    if node.type in COMMENT_TYPES:
        if not is_allowed(source[node.start_byte : node.end_byte], node.start_byte):
            found.append(removal_range(node))
        return
    for child in node.children:
        collect(child, source, found)


def removal_range(node: Node) -> tuple[int, int]:
    parent = node.parent
    if (
        parent is not None
        and parent.type in ("jsx_expression", "jsx_expression_container")
        and all(c.type in COMMENT_TYPES or c.type in ("{", "}") for c in parent.children)
    ):
        return parent.start_byte, parent.end_byte
    end = node.end_byte
    while (
        end > node.start_byte
        and node.text[end - node.start_byte - 1 : end - node.start_byte] == b"\n"
    ):
        end -= 1
    return node.start_byte, end


def strip(source: bytes, ranges: list) -> bytes:
    out = bytearray(source)
    for start, end in sorted(set(ranges), reverse=True):
        line_start = out.rfind(b"\n", 0, start) + 1
        line_end = out.find(b"\n", end)
        if line_end == -1:
            line_end = len(out)
        before = out[line_start:start]
        after = out[end:line_end]
        if before.strip() == b"" and after.strip() == b"":
            prev_blank = out[max(line_start - 2, 0) : line_start] == b"\n\n"
            next_start = line_end + 1
            next_end = out.find(b"\n", next_start)
            next_line = out[next_start : next_end if next_end != -1 else len(out)]
            drop_end = line_end + 1 if line_end < len(out) else line_end
            if (prev_blank and next_line.strip() == b"" and next_end != -1) or (
                line_start == 0 and next_line.strip() == b"" and next_end != -1
            ):
                drop_end = next_end + 1
            del out[line_start:drop_end]
        else:
            trim = start
            while trim > line_start and out[trim - 1] in b" \t":
                trim -= 1
            if after.strip() == b"":
                del out[trim:line_end]
            else:
                left, right = start, end
                if right < len(out) and out[right] in b" \t":
                    while left > line_start and out[left - 1] in b" \t":
                        left -= 1
                del out[left:right]
    return bytes(out)


def tracked_under(paths: list[str]) -> list[Path]:
    listed = subprocess.run(
        ["git", "ls-files", "-z", "--", *paths], check=True, capture_output=True
    ).stdout.split(b"\0")
    return [Path(p.decode()) for p in listed if p]


def candidates(paths: list[str]) -> list[Path]:
    files = tracked_under(paths)
    return [
        f
        for f in files
        if f.suffix in LANGUAGES and not str(f).startswith(EXCLUDED_PREFIXES) and f.is_file()
    ]


def line_of(source: bytes, offset: int) -> int:
    return source.count(b"\n", 0, offset) + 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fix", action="store_true")
    parser.add_argument("paths", nargs="*")
    args = parser.parse_args()
    violations = 0
    for path in candidates(args.paths):
        source = path.read_bytes()
        tree = get_parser(LANGUAGES[path.suffix]).parse(source)
        found: list = []
        collect(tree.root_node, source, found)
        if not found:
            continue
        if args.fix:
            path.write_bytes(strip(source, found))
            continue
        violations += len(found)
        for start, _ in found:
            print(f"{path}:{line_of(source, start)}: comment")
    if violations:
        print(
            f"{violations} comments found; run scripts/comments.sh --fix",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
