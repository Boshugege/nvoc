"""macOS shims in the TUI: one transport, two fan policies, no curve table.

The pane used to offer NVAPI and NVML on every host. On macOS neither exists,
and the selector is not cosmetic -- it is the transport every write is
addressed to, so the pane offered choices that could only fail, and the failure
read like a driver fault instead of "this card has no such surface".
"""

from __future__ import annotations

from types import SimpleNamespace

import pytest
from nvoc_tui.controllers.overclock import OverclockController
from nvoc_tui.platform import (
    MACOS_BACKEND,
    MACOS_FAN_POLICY_REASON,
    api_default,
    api_options,
    fan_curve_available,
    fan_policy_default,
    fan_policy_options,
    macos_duty_seed,
    macos_fan_request,
)

from test_controllers import FakeApp


class MinimalApp(FakeApp):
    """FakeApp without the overclock widgets a fan-only test never touches."""

    def __init__(self) -> None:
        super().__init__()


class StubWidget:
    """Just enough widget surface for the adapter (value/set_options/display)."""

    def __init__(self, value=None) -> None:
        self.value = value
        self.options = []
        self.display = True

    def set_options(self, options) -> None:
        self.options = list(options)


def test_macos_offers_exactly_one_transport():
    assert api_options(True) == [("RM", MACOS_BACKEND)]
    assert api_default(True) == MACOS_BACKEND
    # Elsewhere the two NV transports stay exactly as they were.
    assert api_options(False) == [("NVAPI", "nvapi"), ("NVML", "nvml")]
    assert api_default(False) == "nvapi"


def test_macos_fan_policies_are_the_three_that_exist():
    # fanstop is the firmware's zero-RPM mode, which is a different request from
    # pinning 0 percent: one asks the cooler to stop, the other pins a duty.
    assert fan_policy_options(True) == [
        ("manual", "manual"),
        ("fanstop", "fanstop"),
        ("auto", "auto"),
    ]
    assert fan_policy_default(True) == "manual"
    # The NVAPI cooler policies are untouched off macOS.
    assert fan_policy_options(False) == [
        ("contin.", "continuous"),
        ("curve", "curve"),
    ]
    assert fan_policy_default(False) == "continuous"


def test_the_curve_editor_has_nothing_to_talk_to_on_macos():
    assert fan_curve_available(True) is False
    assert fan_curve_available(False) is True


@pytest.mark.parametrize(
    ("policy", "expected"),
    [
        ("manual", "manual"),
        # The software continuous policy is the manual level under another
        # name, so it keeps working rather than becoming an error.
        ("continuous", "manual"),
        ("fanstop", "fanstop"),
        ("auto", "auto"),
        (None, "manual"),
        ("MANUAL", "manual"),
    ],
)
def test_policies_map_onto_the_macos_requests(policy, expected):
    assert macos_fan_request(policy) == expected


def test_the_curve_policy_is_refused_with_the_reason_not_downgraded():
    # Silently applying "curve" as a fixed duty would pin the fan at whatever
    # number happened to be in the level box -- a different outcome than asked.
    with pytest.raises(ValueError) as excinfo:
        macos_fan_request("curve")
    message = str(excinfo.value)
    assert "curve" in message
    assert "no " in message and "temperature" in message
    assert MACOS_FAN_POLICY_REASON in message


def test_the_duty_seed_reads_the_duty_the_driver_holds():
    # target_percent is what get-fan-info prints and what the driver stores.
    assert macos_duty_seed({"target_percent": 44.99969482421875}) == 45
    assert macos_duty_seed({"target_percent": None, "current_percent": 55}) == 55
    # In auto mode both can read 0 while the firmware spins the fan; a zero
    # seed is only taken when the report says the duty is manual.
    assert macos_duty_seed({"target_percent": 0.0, "mode": "auto"}) is None
    assert macos_duty_seed({"target_percent": 0.0, "mode": "manual"}) == 0
    assert macos_duty_seed({}) is None
    assert macos_duty_seed(None) is None
    # Booleans are ints in Python and must not become a duty.
    assert macos_duty_seed({"target_percent": True}) is None


def test_apply_fan_sends_the_mapped_policy_to_the_rm_backend():
    app = MinimalApp()
    sent = []

    class Native:
        def set_fan(self, gpu, backend, fan_id, policy, level):
            sent.append((gpu, backend, fan_id, policy, level))

    controller = OverclockController(app)
    message = controller.apply_fan(
        Native(), "0x0000", MACOS_BACKEND, "all", False, "continuous", 40
    )
    assert sent == [("0x0000", MACOS_BACKEND, "all", "manual", 40)]
    # The message says what was actually written and why it is not "continuous".
    assert "manual level 40%" in message
    assert "requested continuous" in message


def test_apply_fan_reset_hands_the_fan_back_to_the_firmware():
    app = MinimalApp()
    sent = []

    class Native:
        def set_fan(self, gpu, backend, fan_id, policy, level):
            sent.append((policy, level))

    controller = OverclockController(app)
    controller.apply_fan(Native(), "0x0000", MACOS_BACKEND, "all", True, "curve", 40)
    assert sent == [("auto", 0)]


def test_apply_fan_leaves_the_nvapi_path_alone():
    app = MinimalApp()
    sent = []

    class Native:
        def set_fan(self, gpu, backend, fan_id, policy, level):
            sent.append((backend, policy, level))

    controller = OverclockController(app)
    controller.apply_fan(Native(), "0x0000", "nvapi-cooler", "1", False, "curve", 70)
    assert sent == [("nvapi-cooler", "curve", 70)]


def test_the_macos_surface_seeds_the_pane_from_the_drivers_duty():
    app = MinimalApp()
    widgets = {
        "#fan-id": StubWidget("all"),
        "#fan-policy": StubWidget("continuous"),
        "#fan-level": StubWidget("60"),
        "#fan-curve-editor": StubWidget(),
        "#fan-controls": StubWidget(),
        "#fan-actions": StubWidget(),
    }
    app.widgets.update(widgets)
    app.native_service = SimpleNamespace(action_state=SimpleNamespace(running=False))
    controller = OverclockController(app)
    controller._fan_backend = lambda: MACOS_BACKEND

    controller._on_fan_surface(
        "0x0000",
        {
            "count": 1,
            "target_percent": 55.00030517578125,
            "current_percent": 55,
            "mode": "manual",
            "max_rpm": 2800,
        },
        None,
    )

    assert widgets["#fan-id"].options == [("All", "all"), ("Fan 1", "1")]
    assert widgets["#fan-policy"].options == [
        ("manual", "manual"),
        ("fanstop", "fanstop"),
        ("auto", "auto"),
    ]
    assert widgets["#fan-policy"].value == "manual"
    assert widgets["#fan-level"].value == "55"
    # The curve table does not exist on this transport.
    assert widgets["#fan-curve-editor"].display is False
    assert any("native RM" in line for line in app.logs)
    assert any("2800 rpm" in line for line in app.logs)


def test_a_cardless_report_disables_the_pane_instead_of_showing_zeros():
    app = MinimalApp()
    widgets = {"#fan-controls": StubWidget(), "#fan-actions": StubWidget()}
    app.widgets.update(widgets)
    controller = OverclockController(app)
    controller._fan_backend = lambda: MACOS_BACKEND

    controller._on_fan_surface("0x0000", {"count": 0}, None)

    assert widgets["#fan-controls"].disabled is True
    assert "0x0000" in controller._fanless_gpus
    assert any("no cooler" in line for line in app.logs)
