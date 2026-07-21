# screen_objects

`screen_objects` is a Rust-powered Python module for Android screen automation. It finds saved image samples on the current device screen, then taps, swipes, waits for, counts, or calibrates those screen objects through ADB.

The project is built with [PyO3](https://pyo3.rs/) and [maturin](https://www.maturin.rs/). It exposes a Python extension module named `screen_objects` and also builds as a Rust library.

## Features

- Load template images from a samples directory.
- Detect whether an object exists on the Android screen.
- Tap, repeated-tap, tap every match, or tap the nth match.
- Wait for an object to appear and tap it.
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

device_config(Path("/path/to/adb"), ip=None)

regions = get_regions(Path("assets/regions"))
objects = get_objects(Path("assets/objects"), Path("assets/regions"))

objects["start_button"].calibrate()

if objects["start_button"].exists():
    objects["start_button"].tap()

objects["reward_badge"].waitap(timeout=10.0)
objects["close_icon"].swipe(Direction.Left, SwipeSpeed.Normal, duration=0.4)
```

Pass an IP address to `device_config` when using wireless debugging:

```python
device_config(Path("/path/to/adb"), ip="192.168.1.20")
```

If no device is connected, the module prompts for the wireless debugging port.

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
device_config(adb: Path, ip: str | None) -> None
reset_screen() -> None
screenshot() -> None
back() -> None
home() -> None
```

`screenshot()` writes the current device screenshot to `screen.png`.

`reset_screen()` clears the cached screenshot so the next lookup captures a fresh screen.

`back()` sends Android's back key event and resets the screenshot cache.

`home()` sends Android's home key event and resets the screenshot cache.

### ScreenObject

```python
exists() -> bool
tap() -> bool
waitap(timeout: float = 60.0) -> bool
swipe(dir: Direction, speed: SwipeSpeed, duration: float) -> bool
tap_nth(n: int) -> bool
count() -> int
tap_each() -> None
spam_tap(n: int, interval: float) -> bool
wait(timeout: float = 60.0) -> bool
calibrate(fixed: bool = False, region: str | None = None, n: int | None = None) -> None
```

Methods that return `bool` return `False` when the object is not found before acting.

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
