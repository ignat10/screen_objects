# screen_objects

`screen_objects` is a Python extension for Android UI automation. It finds saved image samples on
the current device screen, then checks, taps, swipes, counts, and calibrates those screen objects
through ADB.

It is implemented in Rust with [PyO3](https://pyo3.rs/) and built with
[maturin](https://www.maturin.rs/).

## Requirements

- Python 3.8 or newer
- Rust nightly, as configured in `rust-toolchain.toml`
- [maturin](https://www.maturin.rs/)
- Android Debug Bridge (`adb`)
- An Android device or emulator with USB or wireless debugging enabled

## Install

For local development:

```bash
pip install maturin
maturin develop
```

To build a wheel:

```bash
maturin build --release
```

## Quick start

Put reference images in an objects directory. Each filename stem becomes an object name.

```text
assets/
  objects/
    start_button.png
    close_icon.png
```

Configure the device before performing any device operation, then load the objects:

```python
from pathlib import Path

from screen_objects import Direction, SwipeSpeed, device_config, get_objects

device_config(serial="emulator-5554")
objects = get_objects(Path("assets/objects"))

if objects["start_button"].tap(timeout=10.0):
    objects["close_icon"].swipe(Direction.Left, SwipeSpeed.Normal, duration=0.4)
```

`device_config()` uses `adb` from `PATH` by default. Pass the executable path when necessary:

```python
device_config(Path("/path/to/adb"), serial="192.168.1.20:5555")
```

Use the serial shown by `adb devices` when more than one device is connected.

## Samples and calibration

`get_objects(path)` loads every image below `path` and returns a dictionary of `ScreenObject`
instances. Image stems must be unique, including across subdirectories. The function creates or
updates `objects.json` next to the objects directory to store calibration data.

By default, an object is searched for anywhere on the screen. Calibrating it can improve matching:

```python
objects["start_button"].calibrate()
```

Calibrate while the object is visible. This records a matching tolerance and continues to search
the whole screen. Use `fixed=True` when the object always occupies the same screen position:

```python
objects["close_icon"].calibrate(fixed=True)
```

### Regions

Regions constrain object matching to part of the screen. Store their reference images separately,
calibrate the region first, then load objects with the same regions directory:

```python
from screen_objects import get_regions

regions_dir = Path("assets/regions")
objects_dir = Path("assets/objects")

regions = get_regions(regions_dir)
regions["main_panel"].calibrate()

objects = get_objects(objects_dir, regions_dir)
objects["reward_badge"].calibrate(region="main_panel")
```

Region data is saved as `regions.json` beside the regions directory. Call `get_regions()` before
`get_objects()` when an application needs both collections.

## Matching and actions

Lookups use a cached screenshot. Actions that change the device through this module clear that
cache. If the screen changes outside this module, call `reset_screen()` before the next lookup.

`timeout` is measured in seconds. The default, `0.0`, performs an immediate lookup. A positive
value waits for a match until the timeout expires.

`index` selects a specific match and is zero-based: `index=0` selects the first match. It can be
used with `exists()`, `tap()`, `swipe()`, and `calibrate()`. Objects calibrated with
`fixed=True` have one known position, so they do not support `index`, `count()`, or
`tap_each()`.

```python
# Check for a match for up to five seconds.
if objects["reward_badge"].exists(timeout=5.0):
    # Tap the second match three times, waiting 0.2 seconds between taps.
    objects["reward_badge"].tap(index=1, times=3, interval=0.2)

# Tap every match in the current screenshot and receive the number tapped.
tapped = objects["collect_button"].tap_each()
```

### Required actions

Every lookup or action that can return `False` has a `force_*` form. It raises `RuntimeError` and
saves the current screenshot to `screen-<device-serial>.png` when no match is found.

```python
objects["start_button"].force_tap(timeout=10.0)
objects["close_icon"].force_exists()
```

## API reference

### Object loading

```python
get_objects(objects_dir: Path, regions_dir: Path | None = None) -> dict[str, ScreenObject]
get_regions(regions_dir: Path) -> dict[str, ScreenRegion]
```

### `ScreenObject`

```python
exists(timeout: float = 0.0, index: int | None = None) -> bool
force_exists(timeout: float = 0.0, index: int | None = None) -> None

tap(
    timeout: float = 0.0,
    index: int | None = None,
    times: int = 1,
    interval: float = 0.0,
) -> bool
force_tap(
    timeout: float = 0.0,
    index: int | None = None,
    times: int = 1,
    interval: float = 0.0,
) -> None

swipe(
    dir: Direction,
    speed: SwipeSpeed,
    duration: float,
    index: int | None = None,
) -> bool
force_swipe(
    dir: Direction,
    speed: SwipeSpeed,
    duration: float,
    index: int | None = None,
) -> None

count() -> int
tap_each() -> int
calibrate(
    fixed: bool = False,
    region: str | None = None,
    index: int | None = None,
) -> None
```

`tap()` returns `False` if the object is not found before `timeout`. `times=0` performs no taps
and returns `True` when the object is found; `interval` is the delay between repeated taps.
`swipe()` begins at the matched point and returns
`False` when there is no match. `count()` returns the number of current matches, and
`tap_each()` returns the number of taps sent.

### Device functions

```python
device_config(
    adb: Path = Path("adb"),
    serial: str | None = None,
    app: str | None = None,
) -> None
start_app() -> None
close_app() -> None

reset_screen() -> None
screenshot() -> None
tap_center() -> None
swipe_center(dir: Direction, speed: SwipeSpeed, duration: float) -> None
back() -> None
home() -> None
write(text: str) -> None
```

Pass `app` to `device_config()` to select an Android package by name. `start_app()` launches it,
and `close_app()` force-stops it.

`screenshot()` saves the cached or newly captured screen to `screen-<device-serial>.png`.
`tap_center()`, `swipe_center()`, `back()`, and `home()` send their respective device action.
`write()` enters text into the focused Android field. Call `reset_screen()` after an external or
text-entry action when a fresh lookup is needed.

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

Build the extension into the active Python environment with:

```bash
maturin develop
```
