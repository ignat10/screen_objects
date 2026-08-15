# screen_objects

`screen_objects` is a Rust-powered Python module for Android screen automation. It finds saved image samples on the current device screen, then taps, swipes, waits for, counts, or calibrates those screen objects through ADB.

The project is built with [PyO3](https://pyo3.rs/) and [maturin](https://www.maturin.rs/). It exposes a Python extension module named `screen_objects` and also builds as a Rust library.

## Features

- Load template images from a samples directory.
- Detect whether an object exists on the Android screen.
- Tap, repeated-tap, tap every match, or tap the nth match.
- Wait for an object to appear and tap it.
- Raise an error and save a diagnostic screenshot when a required object is not found.
- Swipe from a detected object in a chosen direction and speed.
- Calibrate objects and optional screen regions.
- Cache screenshots until an action changes the screen.
- Save calibration data to JSON files next to your sample directories.

## Requirements

- Python 3.8 or newer.
- Rust nightly, as configured in `rust-toolchain.toml`.
- `maturin` for building/installing the Python module.
- Android Debug Bridge (`adb`).
- An Android device or emulator with USB debugging or wireless debugging enabled.

## Installation

For local development, create or activate a Python environment and install the module in editable mode:

```bash
pip install maturin
maturin develop
```

To build a wheel:

```bash
maturin build --release
```

## Basic Usage

Create a directory of image samples. Each file name becomes the key for a `ScreenObject`.

```text
assets/
  objects/
    start_button.png
    close_icon.png
    reward_badge.png
  regions/
    main_panel.png
```

Then configure ADB and load the objects:

```python
from pathlib import Path

from screen_objects import (
    Direction,
    SwipeSpeed,
    device_config,
    get_objects,
    get_regions,
)

device_config(Path("/path/to/adb"), serial="emulator-5554")

regions = get_regions(Path("assets/regions"))
objects = get_objects(Path("assets/objects"), Path("assets/regions"))

objects["start_button"].calibrate()

if objects["start_button"].exists():
    objects["start_button"].tap()

objects["reward_badge"].waitap(timeout=10.0)
objects["close_icon"].swipe(Direction.Left, SwipeSpeed.Normal, duration=0.4)
```

Pass the device serial reported by `adb devices` to select a specific device:

```python
device_config(Path("/path/to/adb"), serial="emulator-5554")
```

For a wirelessly connected device, the serial is typically an address with a port, such as
`"192.168.1.20:5555"`.

## Calibration Data

`get_objects()` scans a directory and returns a dictionary of `ScreenObject` instances. It also creates or updates an `objects.json` file next to the objects directory.

For example, loading `assets/objects` stores object calibration data at:

```text
assets/objects.json
```

`get_regions()` does the same for regions and stores data at:

```text
assets/regions.json
```

Object calibration records:

- fixed coordinates, when `calibrate(fixed=True)` is used.
- an optional region name, when `calibrate(region="name")` is used.
- the image matching tolerance.

Regions must be calibrated before objects can reliably use them:

```python
regions = get_regions(Path("assets/regions"))
regions["main_panel"].calibrate()

objects = get_objects(Path("assets/objects"), Path("assets/regions"))
objects["reward_badge"].calibrate(region="main_panel")
```

## Python API

### Module Functions

```python
get_objects(objects_dir: Path, regions_dir: Path | None = None) -> dict[str, ScreenObject]
get_regions(regions_dir: Path) -> dict[str, ScreenRegion]
device_config(adb: Path = Path("adb"), serial: str | None = None, app: str | None = None) -> None
start_app() -> None
close_app() -> None
reset_screen() -> None
tap_center() -> None
swipe_center(dir: Direction, speed: SwipeSpeed, duration: float) -> None
screenshot() -> None
back() -> None
home() -> None
```

`screenshot()` writes the current device screenshot to `screen.png`.

`reset_screen()` clears the cached screenshot so the next lookup captures a fresh screen.

`tap_center()` taps the center of the configured Android device screen and resets the screenshot cache.

`swipe_center()` swipes from the center of the configured Android device screen in the requested direction and resets the screenshot cache.

`back()` sends Android's back key event and resets the screenshot cache.

`home()` sends Android's home key event and resets the screenshot cache.

Pass `app` to `device_config()` to configure an Android package by name. `start_app()` and
`close_app()` then start and force-stop that package.

Pass `serial` to select a device from the output of `adb devices`.

### ScreenObject

```python
exists() -> bool
tap() -> bool
force_tap() -> None
waitap(timeout: float = 60.0) -> bool
force_waitap(timeout: float = 60.0) -> None
swipe(dir: Direction, speed: SwipeSpeed, duration: float) -> bool
force_swipe(dir: Direction, speed: SwipeSpeed, duration: float) -> None
tap_nth(n: int) -> bool
force_tap_nth(n: int) -> None
count() -> int
tap_each() -> None
spam_tap(n: int, interval: float) -> bool
force_spam_tap(n: int, interval: float) -> None
wait(timeout: float = 60.0) -> bool
force_wait(timeout: float = 60.0) -> None
calibrate(fixed: bool = False, region: str | None = None, n: int | None = None) -> None
debug(point: Sequence[int]) -> None
```

Methods that return `bool` return `False` when the object is not found before acting.

`force_*` variants raise `RuntimeError` instead of returning `False`. They save the current
screen to `screen.png` to help diagnose a missing object.

### ScreenRegion

```python
calibrate() -> None
```

### Enums

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

Run Rust tests with:

```bash
cargo test
```

Build the Python extension locally with:

```bash
maturin develop
```

The release workflow builds wheels on tagged pushes matching `v*`.
