#!/usr/bin/env python3
"""Extract the live NVOC control method table from an NVRM kext binary.

Why this exists
---------------
The macOS RM is built from the open kernel modules (NVRM prints
``NVIDIA UNIX Open Kernel Module for <arch> <version> Release Build``), so the
client-visible control surface is exactly the compiled ``NVOC_EXPORTED_METHOD_DEF``
arrays.  The public OGKM source tree ships those arrays for the *public*
commands only; any command that is not in the shipped source is therefore not
in the driver either.  Reading the arrays straight out of the loaded binary
settles that question with a primary source instead of a header grep.

Layout (x86_64 kext, ``NVOC_EXPORTED_METHOD_DEF``, 40 bytes per entry):
    +0x00  pFunc        u64   (host VA / file offset for __TEXT symbols)
    +0x08  flags        u32
    +0x0c  accessRight  u32
    +0x10  methodId     u32   class(31:16) legacy(15:14) category(13:8) index(7:0)
    +0x14  paramSize    u32
    +0x18  pClassInfo   u64
    +0x20  funcName     u64

The command-id encoding is from ``ctrl/ctrlxxxx.h``:
``NVXXXX_CTRL_CMD_CLASS 31:16``, ``_CATEGORY 13:8`` (6 bits), ``_INDEX 7:0``,
and ``NVxxxx_CTRL_LEGACY_NON_PRIVILEGED (0x80)`` / ``_PRIVILEGED (0xC0)`` occupy
bits 15:14, i.e. they are *not* part of the category field.

Usage:
    extract-nvrm-method-table.py /Library/Extensions/NVRM.kext/Contents/MacOS/NVRM \
        /path/to/open-gpu-kernel-modules-<ver> > table.tsv

The second argument is optional and is used only to attach human-readable
function names from the generated ``g_*_nvoc.c`` tables.

Read-only: the tool only reads files.
"""

import argparse
import glob
import os
import re
import struct
import sys

ENTRY_SIZE = 40
U32 = struct.Struct("<I")
U64 = struct.Struct("<Q")


def extract(binary_path):
    """Return {methodId: (paramSize, pClassInfo, fileOffset)} from the binary."""
    data = open(binary_path, "rb").read()
    limit = len(data)
    seen = {}
    for off in range(0, limit - ENTRY_SIZE, 4):
        method_id = U32.unpack_from(data, off + 16)[0]
        param_size = U32.unpack_from(data, off + 20)[0]
        class_ptr = U64.unpack_from(data, off + 24)[0]
        if (method_id & 0xFFFF0000) != 0x20800000:
            continue
        if not 0 < param_size < 8_000_000:
            continue
        if not 0x1000 <= class_ptr < limit:
            continue
        # NOTE (2026-10-17): there used to be a ``flags == 0 or flags >
        # 0xFFFFFFF`` guard here.  It dropped 4 real rows (604 reported
        # instead of 608 -- 0x20801350/0x20801351/0x20802204/0x20803124, the
        # first with flags == 0).  None of them is in the private clock /
        # voltage families, so the "the loaded host table has no row for
        # 0x20809019/0x2080901b/0x2080b201/..." result is unaffected: a
        # relaxed scan (no flags guard) still finds exactly ONE CLK row
        # (0x20801037) and NO row for any 0x208090xx / 0x2080b2xx id.
        seen.setdefault(method_id, (param_size, class_ptr, off))
    return seen


def names_from_ogkm(ogkm_root):
    """Map methodId -> exported function name using the generated sources."""
    names = {}
    pattern = re.compile(r'/\*func=\*/\s+"([A-Za-z0-9_]+)"')
    for path in glob.glob(os.path.join(ogkm_root, "src/nvidia/generated/g_*_nvoc.c")):
        text = open(path, errors="ignore").read()
        for match in pattern.finditer(text):
            window = text[max(0, match.start() - 900) : match.start()]
            ids = re.findall(r"0x(20[0-9a-fA-F]{6})u", window)
            if ids:
                names.setdefault(int(ids[-1], 16), match.group(1))
    return names


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("binary", help="path to the NVRM kext executable")
    ap.add_argument(
        "ogkm", nargs="?", help="open-gpu-kernel-modules source root (optional)"
    )
    args = ap.parse_args()

    entries = extract(args.binary)
    names = names_from_ogkm(args.ogkm) if args.ogkm else {}
    print("# methodId\tparamSize\tclassPtr\toffset\tname")
    for method_id in sorted(entries):
        param_size, class_ptr, off = entries[method_id]
        print(
            "0x%08x\t%d\t0x%x\t0x%x\t%s"
            % (method_id, param_size, class_ptr, off, names.get(method_id, ""))
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
