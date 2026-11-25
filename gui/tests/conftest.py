from __future__ import annotations

import sys
from pathlib import Path

import pytest

GUI_ROOT = Path(__file__).resolve().parents[1]
if str(GUI_ROOT) not in sys.path:
    sys.path.insert(0, str(GUI_ROOT))


@pytest.fixture
def non_macos_host(monkeypatch):
    """Pretend this host is not macOS.

    The NVAPI/NVML fan tests describe a transport this tree only offers off
    macOS. They are written against ``sys.platform`` through
    :func:`src.platform.is_macos`, so on a macOS host they would fail for the
    host they ran on rather than for the code they describe. The fixture makes
    that assumption explicit instead of leaving it to whichever machine runs
    the suite.
    """
    monkeypatch.setattr(sys, "platform", "linux")


@pytest.fixture(autouse=True)
def _close_matplotlib_figures():
    """Chart tests build real Agg figures through pyplot and never revisit
    them; without an explicit close they accumulate across the session until
    matplotlib's 20-open-figures RuntimeWarning fires on every later test."""
    yield
    import matplotlib.pyplot as plt

    plt.close("all")
