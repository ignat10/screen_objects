# screen_objects

`screen_objects` is a Rust-powered Python extension for Android screen automation. It captures a device screen through ADB, finds saved image samples, and performs actions such as tapping, swiping, waiting, counting, and calibration.

The project uses [PyO3](https://pyo3.rs/) and [maturin](https://www.maturin.rs/).

## Features

- Work with multiple connected Android devices.
- Match image samples against a cached device screenshot.
- Tap, repeatedly tap, tap every match, or tap the nth match.
- Wait for an object and optionally tap it.
- Swipe from an object or from the center of the screen.
- Calibrate objects and named screen regions.
- Start and stop a configured Android application.
- Save diagnostic screenshots when required objects are missing.

## Requirements

- Python 3.8 or newer.
- Rust nightly, as configured by `rust-toolchain.toml`.
- [maturin](https://www.maturin.rs/).
- Android Debug Bridge (`adb`).
- At least one Android device or emulator with debugging enabled.

## Installation

Install the extension into the active Python environment:

```bash
pip install maturin
maturin develop
```

Build a release wheel with:

```bash
maturin build --release
```

## Sample layout

Each image filename becomes its object or region name:

```text
assets/
  objects/
    start_button.png
    close_icon.png
    reward_badge.png
  regions/
    main_panel.png
```

Calibration data is stored next to those directories:

```text
assets/
  objects.json
  regions.json
```

## Basic usage

Configure samples first, then discover connected devices:

```python
from pathlib import Path

from screen_objects import (
    Direction,
    SwipeSpeed,
    config_objects,
    config_regions,
    get_devices,
)

regions_dir = Path("assets/regions")
objects_dir = Path("assets/objects")

config_regions(regions_dir)
config_objects(objects_dir, regions_dir)

devices = get_devices(Path("adb"), app="com.example.app")
device = devices[0]

if device.exists("start_button"):
    device.tap("start_button")

device.waitap("reward_badge", timeout=10.0)
device.swipe("close_icon", Direction.Left, SwipeSpeed.Normal, duration=0.4)
```

The direct `Device` methods are the simplest API. Indexed access is also available when repeatedly working with one object:

```python
start_button = device["start_button"]

start_button.wait(timeout=10.0)
start_button.tap()
```

The indexed object keeps the original device alive and uses the same screenshot cache.

## Calibration

Calibrate a region using a device screenshot:

```python
config_regions(Path("assets/regions"))
device.calibrate_region("main_panel")
```

After region calibration, configure objects with the regions directory and calibrate an object:

```python
config_objects(Path("assets/objects"), Path("assets/regions"))

device.calibrate(
    "reward_badge",
    fixed=False,
    region="main_panel",
    n=None,
)
```

The indexed equivalent provides defaults for the optional calibration arguments:

```python
device["reward_badge"].calibrate(region="main_panel")
```

## Device API

Object operations accept the configured image name as their first argument:

```python
device.exists(name) -> bool
device.wait(name, timeout=60.0) -> bool
device.force_wait(name, timeout=60.0) -> None
device.tap(name) -> bool
device.force_tap(name) -> None
device.waitap(name, timeout=60.0) -> bool
device.force_waitap(name, timeout=60.0) -> None
device.spam_tap(name, n, interval) -> bool
device.force_spam_tap(name, n, interval) -> None
device.swipe(name, direction, speed, duration) -> bool
device.force_swipe(name, direction, speed, duration) -> None
device.tap_nth(name, n) -> bool
device.force_tap_nth(name, n) -> None
device.tap_each(name) -> None
device.count(name) -> int
device.calibrate(name, fixed, region, n) -> None
device.calibrate_region(name) -> None
```

Device-level operations:

```python
device.tap_center() -> None
device.swipe_center(direction, speed, duration) -> None
device.start_app() -> None
device.close_app() -> None
device.back() -> None
device.home() -> None
device.reset_screen() -> None
device.save_screen() -> None
```

Methods returning `bool` return `False` when no matching object is found. The `force_*` variants raise `RuntimeError` and save the current screen to `screen.png`.

Screenshots are cached until an action resets the cache. Call `reset_screen()` explicitly when the screen changed outside this library.

## Device discovery

```python
get_devices(adb=Path("adb"), app=None) -> list[Device]
```

The optional `app` value is matched against installed Android package names. When supplied, `start_app()` and `close_app()` operate on the matching package.

Only devices reported by `adb devices` with the `device` status are returned.

## Enums

```python
Direction.Left
Direction.Right
Direction.Up
Direction.Down

SwipeSpeed.Slow
SwipeSpeed.Normal
SwipeSpeed.Fast
SwipeSpeed.Turbo
```

## Development

Format and test the Rust code with:

```bash
cargo fmt --check
cargo test
```

Build the Python extension locally with:

```bash
maturin develop
```

The release workflow builds wheels for tags matching `v*`.
