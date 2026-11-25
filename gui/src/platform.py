"""What this host can be asked to do, as pure functions.

The GUI was written against NVAPI and NVML, and its API dropdowns are not
cosmetic: the selected transport is what every write is addressed to. On macOS
neither library exists -- the only route is the native NullMoth RM backend,
which reads telemetry and drives four families (graphics/memory additive
offsets, the board power limit, cooler0 fan percent plus firmware auto, and the
acoustic temperature target). It has no fan-curve table, no ClkDomains fabric
records and no V/F voltage plane.

So on macOS the fan surface is exactly two policies (manual, auto) on one
transport, and the curve editor has nothing to talk to. These helpers are the
single place that decides; the TUI has a deliberate counterpart in
``nvoc_tui/platform.py``, and both mirror the same measured facts rather than
each guessing.
"""

from __future__ import annotations

import sys

MACOS_BACKEND = "macos-rm"

#: The label the panes show for the single macOS transport.
MACOS_API_LABEL = "RM"

#: Said when a policy the driver cannot implement is asked for anyway.
MACOS_FAN_POLICY_REASON = (
    "the driver exposes cooler0 percent (manual) and firmware auto; it has no "
    "temperature→rpm curve table to read or write, so the software continuous "
    "policy and the curve editor do not exist on this platform"
)

#: Policies the macOS transport can apply, in dropdown order.
#: ``fanstop`` is the firmware's own zero-RPM mode (control-record mode 2),
#: which is a different request from pinning 0 percent: one asks the cooler to
#: stop, the other pins a duty of zero.
#: ``rpm`` is a physical target, translated through the cooler's published
#: scale; the pane's number box carries rpm instead of percent under it.
MACOS_FAN_POLICIES = ["manual", "fanstop", "rpm", "auto"]

#: Pane policy -> the macOS request it means. ``continuous`` is the manual
#: level under another name (the NVAPI cooler family's TemperatureContinuous
#: applies the level), so it maps onto ``manual``.
_MACOS_POLICY_REQUEST = {
    "manual": "manual",
    "continuous": "manual",
    "fanstop": "fanstop",
    "rpm": "rpm",
    "auto": "auto",
}


def is_macos() -> bool:
    """True when running on macOS (the native RM backend's only host)."""
    return sys.platform == "darwin"


def api_labels(macos: bool | None = None) -> list[str]:
    """Transport labels for an API dropdown.

    Offering "NVAPI"/"NVML" on macOS would be two choices whose only outcome is
    a guaranteed failure at apply time, reported as if the driver were broken.
    """
    macos = is_macos() if macos is None else macos
    return [MACOS_API_LABEL] if macos else ["NVAPI", "NVML"]


def api_label_default(macos: bool | None = None) -> str:
    macos = is_macos() if macos is None else macos
    return MACOS_API_LABEL if macos else "NVAPI"


def backend_for_selection(selection: str | None, macos: bool | None = None) -> str:
    """Map an API dropdown label onto the backend string pynvoc takes."""
    macos = is_macos() if macos is None else macos
    if macos:
        return MACOS_BACKEND
    return (
        "nvml-cooler" if (selection or "").strip().upper() == "NVML" else "nvapi-cooler"
    )


def fan_policy_options(macos: bool | None = None) -> list[str]:
    """Cooler policies the selected transport can actually apply."""
    macos = is_macos() if macos is None else macos
    return list(MACOS_FAN_POLICIES) if macos else ["continuous", "curve"]


def fan_policy_default(macos: bool | None = None) -> str:
    macos = is_macos() if macos is None else macos
    return "manual" if macos else "continuous"


def macos_fan_request(policy: str | None) -> str:
    """Map a pane policy onto the macOS request, or raise with the reason.

    Raising is deliberate: silently applying ``curve`` as the level in the box
    would pin the fan at a duty nobody chose, which is a different (and worse)
    outcome than the requested temperature→rpm table.
    """
    key = (policy or "manual").strip().lower()
    if key in _MACOS_POLICY_REQUEST:
        return _MACOS_POLICY_REQUEST[key]
    raise ValueError(
        f"fan policy {key!r} is not available on this platform: "
        + MACOS_FAN_POLICY_REASON
    )


def macos_duty_seed(info: object) -> int | None:
    """The duty to seed the level control with, from a macOS fan-info payload.

    Measured on the test card: pinned at 40 percent the report carries
    ``target_percent`` 39.9994 (what the driver stored) and ``current_percent``
    45 (rounded status feedback); in auto mode ``target_percent`` is 0.0 while
    ``current_percent`` carries the live duty the firmware chose (57 on that
    card). ``target_percent`` is therefore preferred and ``current_percent`` is
    the fallback, so the slider shows what the cooler is doing in auto and what
    it will keep doing in manual. A zero is only accepted when the report says
    the mode is manual; anything absent stays ``None``, because seeding a
    default-looking number would let an accidental Apply write a duty nobody
    chose.
    """
    if not isinstance(info, dict):
        return None
    target = info.get("target_percent")
    current = info.get("current_percent")
    mode = info.get("mode")
    for value in (target, current):
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            if value > 0 or (mode == "manual" and value == 0):
                return int(round(float(value)))
    return None


def fan_curve_available(macos: bool | None = None) -> bool:
    """Whether the ClientFanPolicies curve editor has anything to talk to."""
    macos = is_macos() if macos is None else macos
    return not macos
