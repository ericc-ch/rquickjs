#!/usr/bin/env python3
"""Fail when a bundled binding file's JS_ATOM_* values disagree with
quickjs-atom.h.

Atom values are positional: JS_ATOM_NULL is 0, each `DEF` in the header adds
one in order, and JS_ATOM_END follows the last. Removing or inserting a DEF
therefore renumbers every later atom, and a stale bundled file then points at
the wrong predefined atoms (upstream removing `concat` shifted
Symbol.iterator and Symbol.unscopables, which broke symbol lookups and every
WebIDL sequence conversion until the bindings were regenerated).

Run from anywhere; exits non-zero and lists the drift.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "sys/quickjs/quickjs-atom.h"
BINDINGS = ROOT / "sys/src/bindings"
CONST = re.compile(r"pub const JS_ATOM_([A-Za-z0-9_]+): _bindgen_ty_\d+ = (\d+);")


def expected_values() -> dict[str, int]:
    names = re.findall(r"^DEF\(([A-Za-z0-9_]+),", HEADER.read_text(), re.M)
    values = {name: index + 1 for index, name in enumerate(names)}
    values["END"] = len(names) + 1
    return values


def main() -> int:
    expected = expected_values()
    failed = False
    files = sorted(BINDINGS.glob("*.rs"))
    for path in files:
        found = {
            match.group(1): int(match.group(2))
            for match in CONST.finditer(path.read_text())
        }
        drift = {
            name: (found.get(name), value)
            for name, value in expected.items()
            if found.get(name) != value
        }
        if drift:
            failed = True
            for name, (got, want) in sorted(drift.items()):
                print(f"{path.name}: JS_ATOM_{name} = {got}, expected {want}")
    if failed:
        print("regenerate with the Update bindings workflow", file=sys.stderr)
        return 1
    print(f"atom tables match quickjs-atom.h across {len(files)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
