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

* `tu116-clkctl-hex-20261017T092536Z.txt` (md5 `8817a92fe8dad405568e58bb599c9797`)
  -- a whole 167499-byte `--clkctl-hex` capture from the same run, kept verbatim
  (nothing is trimmed, so there is no selection rule to argue about).
  `decode-clk-containers.py` reads it in `check_real_capture()`, which is the
  parser's own control: every other check feeds the parser bytes that file also
  produced, so a change to the dump line format or to the payload assembly could
  empty a healthy run's decode without failing anything. The check recovers both
  accepted blocks whole by length (0x20809019 at 12336 B, 0x20809025 at 7188 B),
  asserts the head words, the ten directory rows with their type bytes, the two
  signed 16-bit pairs at rec+44, and the max-VF/offset-window record fields, and
  then drops one dump line to prove the lengths really came off the lines.

  That capture is also where the *negative* half of the layout work lives: the
  file's own decode is what showed 0x20809019's rows to be in directory order
  and 0x20809025's records to match the independently recorded Windows/NVAPI
  tree. Note it predates the current printer, which now also writes `bg=` and
  the changed-dword count on the summary line -- so a future capture will not be
  byte-comparable with this one, which is a reason to keep it, not to update it.
