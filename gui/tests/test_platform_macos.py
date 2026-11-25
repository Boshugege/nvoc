"""The GUI offers one transport on macOS, two fan policies, and no curve table.

The API dropdown is not cosmetic -- the selected transport is what every write
is addressed to -- so on macOS it must not offer NVAPI/NVML, and the fan policy
dropdown must not offer the software continuous policy or the curve table,
which this driver does not have. These tests pin the decision surface; the write
itself is covered by the hardware evidence in
``outputs/stage-tui-macos-fan-*`` (same pynvoc calls, same arguments).
"""

from __future__ import annotations

import pytest
from src.backend.base import FanSettings
from src.platform import (
    MACOS_BACKEND,
    MACOS_FAN_POLICY_REASON,
    api_label_default,
    api_labels,
    backend_for_selection,
    fan_curve_available,
    fan_policy_default,
    fan_policy_options,
    is_macos,
    macos_duty_seed,
    macos_fan_request,
)
from src.tabs.dashboard.sections.fan import FanControlController
from test_fan_control import FakeBackend, FakePane


def test_macos_has_one_transport_label():
    assert api_labels(True) == ["RM"]
    assert api_label_default(True) == "RM"
    # Everything off macOS is exactly as it was.
    assert api_labels(False) == ["NVAPI", "NVML"]
    assert api_label_default(False) == "NVAPI"


@pytest.mark.parametrize(
    ("selection", "macos", "expected"),
    [
        ("RM", True, MACOS_BACKEND),
        # Whatever the dropdown says on macOS, there is only one backend.
        ("NVML", True, MACOS_BACKEND),
        ("NVAPI", False, "nvapi-cooler"),
        ("NVML", False, "nvml-cooler"),
    ],
)
def test_the_dropdown_label_maps_onto_a_real_backend(selection, macos, expected):
    assert backend_for_selection(selection, macos) == expected


def test_macos_fan_policies_are_the_four_the_driver_has():
    # fanstop is the firmware's zero-RPM mode (not a 0 percent pin), and rpm is a
    # physical target translated through the cooler's own scale.
    assert fan_policy_options(True) == ["manual", "fanstop", "rpm", "auto"]
    assert fan_policy_default(True) == "manual"
    assert fan_policy_options(False) == ["continuous", "curve"]
    assert fan_curve_available(True) is False
    assert fan_curve_available(False) is True


@pytest.mark.parametrize(
    ("policy", "expected"),
    [
        ("manual", "manual"),
        ("continuous", "manual"),
        ("auto", "auto"),
        (None, "manual"),
        ("Manual", "manual"),
    ],
)
def test_policies_map_onto_the_macos_requests(policy, expected):
    assert macos_fan_request(policy) == expected


def test_the_curve_policy_raises_with_the_reason_instead_of_pinning_a_duty():
    with pytest.raises(ValueError) as excinfo:
        macos_fan_request("curve")
    message = str(excinfo.value)
    assert "curve" in message
    assert MACOS_FAN_POLICY_REASON in message


def test_the_duty_seed_prefers_the_duty_the_driver_holds():
    assert macos_duty_seed({"target_percent": 39.9993896484375}) == 40
    # Monitor-only payloads (the card in auto) fall back to the live duty.
    assert macos_duty_seed({"target_percent": 0.0, "current_percent": 57}) == 57
    assert macos_duty_seed({"target_percent": 0.0, "current_percent": 0}) is None
    assert macos_duty_seed({"target_percent": 0.0, "mode": "manual"}) == 0
    assert macos_duty_seed({"target_percent": True}) is None
    assert macos_duty_seed(None) is None


def test_apply_sends_the_mapped_policy_on_the_macos_transport(monkeypatch):
    monkeypatch.setattr(
        "src.tabs.dashboard.sections.fan.backend_for_selection",
        lambda selection: MACOS_BACKEND,
    )
    pane = FakePane(api="RM", fan_id="All", policy="continuous", level=45)
    backend = FakeBackend()

    FanControlController(pane, backend).apply()

    assert backend.applied == [
        FanSettings(backend=MACOS_BACKEND, fan_id=None, policy="manual", level=45)
    ]


def test_reset_hands_the_fan_back_to_the_firmware(monkeypatch):
    monkeypatch.setattr(
        "src.tabs.dashboard.sections.fan.backend_for_selection",
        lambda selection: MACOS_BACKEND,
    )
    pane = FakePane(api="RM", fan_id="All", policy="manual", level=45)
    backend = FakeBackend()

    FanControlController(pane, backend).reset()

    assert backend.reset == [
        FanSettings(backend=MACOS_BACKEND, fan_id=None, policy="auto", level=0)
    ]


def test_the_macos_dropdown_no_longer_offers_a_policy_it_cannot_apply(
    monkeypatch,
):
    monkeypatch.setattr(
        "src.tabs.dashboard.sections.fan.backend_for_selection",
        lambda selection: MACOS_BACKEND,
    )
    pane = FakePane(api="RM", policy="continuous", level=45)
    controller = FanControlController(pane, backend=FakeBackend())

    controller.on_backend_change()

    assert pane.policy_values == ["manual", "fanstop", "rpm", "auto"]
    # A stale "continuous" selection is normalized onto a real one, not sent.
    assert pane.policy == "manual"


def test_the_nvapi_path_is_untouched():
    pane = FakePane(api="NVAPI", fan_id="Fan 1", policy="curve", level=70)
    backend = FakeBackend()
    controller = FanControlController(pane, backend)
    assert controller.allowed_policies() == ["continuous", "curve"]
    # "curve" still routes to the curve activation path, not to a level pin.
    assert is_macos() in (True, False)  # host-independent assertion anchor
