"""What this host can actually be asked to do, as pure functions.

The TUI was written against NVAPI/NVML and offers those two transports on
every pane. On macOS neither exists: the only route is the native NullMoth RM
backend, which reads telemetry and drives four families (graphics/memory
additive offsets, the board power limit, cooler0 fan percent plus firmware
auto, and the acoustic temperature target) — and which has no fan curve table,
no ClkDomains fabric records and no V/F voltage plane at all.

Leaving the NVAPI/NVML dropdowns in place there is not a cosmetic problem: the
selected backend is what every write is addressed to, so the pane would offer
choices that can only fail, and the failure would read like a driver fault
instead of "this card has no such surface". The helpers here are the single
place that decides, so the panes and the controllers cannot disagree.
"""

from __future__ import annotations

import sys

MACOS_BACKEND = "macos-rm"

#: Says why the software fan policy and the curve editor are absent on macOS.
#: The firmware owns the curve; this driver exposes cooler0 percent/manual,
#: firmware auto, and the fan-stop threshold — nothing that reads or writes a
#: temperature→rpm table.
MACOS_FAN_POLICY_REASON = (
    "the driver exposes cooler0 percent (manual) and firmware auto; it has no "
    "temperature→rpm curve table to read or write, so the software continuous "
    "policy and the curve editor do not exist on this platform"
)


def is_macos() -> bool:
    """True when running on macOS (the native RM backend's only host)."""
    return sys.platform == "darwin"


def api_options(macos: bool | None = None) -> list[tuple[str, str]]:
    """Transport choices for a pane's API selector.

    macOS gets exactly one: offering NVAPI/NVML there would be two options
    whose only outcome is a guaranteed failure at write time.
    """
    macos = is_macos() if macos is None else macos
    if macos:
        return [("RM", MACOS_BACKEND)]
    return [("NVAPI", "nvapi"), ("NVML", "nvml")]


def api_default(macos: bool | None = None) -> str:
    macos = is_macos() if macos is None else macos
    return MACOS_BACKEND if macos else "nvapi"


def fan_policy_options(macos: bool | None = None) -> list[tuple[str, str]]:
    """Cooler policies the selected transport can actually apply.

    Elsewhere the NVAPI cooler family takes ``continuous`` (the manual level
    driven as a policy) and ``curve`` (hand the fan to the ClientFanPolicies
    table). On macOS both are absent; ``manual`` pins the percent and ``auto``
    hands the fan back to the firmware, which is the real pair of choices.
    """
    macos = is_macos() if macos is None else macos
    if macos:
        # ``fanstop`` is the firmware's own zero-RPM mode (control-record mode
        # 2), which is a different request from pinning 0 percent: one asks the
        # cooler to stop, the other pins a duty of zero.
        return [
            ("manual", "manual"),
            ("fanstop", "fanstop"),
            ("rpm", "rpm"),
            ("auto", "auto"),
        ]
    return [("contin.", "continuous"), ("curve", "curve")]


def fan_policy_default(macos: bool | None = None) -> str:
    macos = is_macos() if macos is None else macos
    return "manual" if macos else "continuous"


#: Pane policies as the select carries them -> the macOS request they mean.
#: ``continuous`` is the manual level under another name, so it lands on
#: ``manual``; ``curve`` has no macOS equivalent and must be refused rather
#: than quietly downgraded to a fixed duty the user did not ask for.
_MACOS_POLICY_REQUEST = {
    "manual": "manual",
    "continuous": "manual",
    "fanstop": "fanstop",
    "rpm": "rpm",
    "auto": "auto",
}


def macos_fan_request(policy: str | None) -> str:
    """Map a pane policy onto the macOS request, or raise with the reason.

    Raising is deliberate: a silent downgrade of ``curve`` to ``manual`` would
    pin the fan at whatever number happened to be in the level box, which is a
    different (and worse) outcome than the one requested.
    """
    key = (policy or "manual").strip().lower()
    if key in _MACOS_POLICY_REQUEST:
        return _MACOS_POLICY_REQUEST[key]
    raise ValueError(
        f"fan policy {key!r} is not available on this platform: "
        + MACOS_FAN_POLICY_REASON
    )


def macos_duty_seed(info: object) -> int | None:
    """The duty to seed the level box with, from a macOS fan-info payload.

    Measured on the test card: pinned at 40 percent the report carries
    ``target_percent`` 39.9994 (the duty the driver stored) and
    ``current_percent`` 45 (rounded status feedback); in auto mode
    ``target_percent`` is 0.0 while ``current_percent`` carries the live duty
    the firmware chose (57 on that card). So ``target_percent`` is preferred and
    ``current_percent`` is the fallback, which makes the box show what the
    cooler is doing in auto and what it will keep doing in manual. A zero is
    only accepted when the report says the mode is manual (a real 0 percent
    pin); anything absent stays ``None``, because seeding a default-looking
    number would let an accidental Apply write a duty nobody chose.
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
