#!/usr/bin/env python3
"""
For each trycmd scenario, add a trailing blank line before any closing
``` that closes a ```console block — needed because trycmd interprets
the absence of trailing whitespace as "expected stdout ends without a
trailing newline," but standard CLI tools (and ours) emit a final \n.

Idempotent: re-running on already-fixed files is a no-op.

See `../aretta-sdk/docs/DECISIONS.md` amendment A2.2.
"""
import sys
from pathlib import Path


def fix(content: str) -> str:
    lines = content.split("\n")
    out = []
    in_console = False
    for i, line in enumerate(lines):
        if line.startswith("```console"):
            in_console = True
            out.append(line)
            continue
        if line.startswith("```") and in_console:
            in_console = False
            # If previous emitted line is not empty, insert a blank line.
            if out and out[-1] != "":
                out.append("")
            out.append(line)
            continue
        out.append(line)
    return "\n".join(out)


def main() -> int:
    changed = 0
    for path in sys.argv[1:]:
        p = Path(path)
        original = p.read_text()
        fixed = fix(original)
        if fixed != original:
            p.write_text(fixed)
            changed += 1
            print(f"  fixed {path}")
    print(f"{changed} file(s) changed of {len(sys.argv) - 1} examined")
    return 0


if __name__ == "__main__":
    sys.exit(main())
