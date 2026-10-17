#!/bin/sh
# stress-macos build script. macOS only: it links Metal and Foundation.
set -e
cd "$(dirname "$0")"
CC="${CC:-cc}"
CFLAGS="-std=c11 -O2 -Wall -Wextra -Werror"
case "$(uname -s)" in
    Darwin) ;;
    *) echo "error: stress-macos requires macOS (Metal)" >&2; exit 1 ;;
esac
"$CC" $CFLAGS metal_check.m -o metal_check -framework Metal -framework Foundation
echo "built $(pwd)/metal_check"
