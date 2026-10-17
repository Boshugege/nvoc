# Fixtures

Byte-exact copies of probe output that a tool in `tools/` checks against, so a
claim stays reproducible without the machine that produced it. Nothing here is
regenerated; each file is identified by the md5 its checker records, and the
checker fails if the bytes drift.

* `tu116-1d-32680-A5.txt` (md5 `bdb3db742ae9bd44990efbffa6eda8e0`) -- the
  `0xA5`-background diff of `0x2080901d` at the accepted size 32680 B, from
  `outputs/stage-gsp-clkctl-20261017T092536Z/clkctl-offsweep-run1-A5.txt`
  (GTX 1650 SUPER / TU116, macOS 15.8.1, NullMoth 2.0.4, RM 610.57.04).
  `decode-clk-containers.py` reads it in `check_measured_repetition()`.

Two caveats that belong with the bytes rather than in prose elsewhere: the diff
printer lists only the first 40 changed dwords (the summary line says 352 of
8170 were changed, first @0, last @3376), so the printed window ends at +184 and
says nothing about the rest of the block; and the values still carry `0xA5`
bytes, so the driver writes sub-dword fields, not whole dwords.
