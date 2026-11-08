use pyo3::exceptions::{
    PyBufferError, PyConnectionAbortedError, PyConnectionError, PyOSError, PyRuntimeError,
    PyValueError,
};
use pyo3::prelude::{PyResult, Python, pyfunction};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::thread::sleep;
use std::time::Duration;
use pixen::Point;
use crate::screen::RGBA_CHANNELS;

static ADB: OnceLock<PathBuf> = OnceLock::new();
pub(crate) static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();
static APP: OnceLock<String> = OnceLock::new();
static ACTIVITY: OnceLock<String> = OnceLock::new();
static DIMENSIONS: OnceLock<Point> = OnceLock::new();

pub(crate) fn get_dimensions() -> PyResult<Point> {
    DIMENSIONS.get().ok_or_else(|| PyValueError::new_err("Dimensions not set. Call device_config before.")).cloned()
}

#[pyfunction]
#[pyo3(signature = (adb = PathBuf::from("adb"), serial = None, app = None))]
pub(super) fn device_config(
    adb: PathBuf,
    serial: Option<String>,
    app: Option<String>,
) -> PyResult<()> {
    ADB.set(adb)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;

    let device = if let Some(serial) = serial {
        connect(&serial)?;
        if !scan()?.contains(&serial) {
            return Err(PyConnectionError::new_err(format!(
                "Failed to connect to '{serial}'."
            )));
        }
        serial
    } else {
        let mut devices = scan()?;
        match devices.len() {
            1 => devices.pop().unwrap(),
            0 => {
                return Err(PyConnectionError::new_err("No adb device found."));
            }
            n => {
                return Err(PyConnectionError::new_err(format!(
                    "Expected exactly one adb device, found {n}: {}, call device_config with serial argument",
                    devices.join(", ")
                )));
            }
        }
    };

    DEVICE_SERIAL
        .set(device)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;
    device_action(&["wait-for-device"])?;
    wait_for_boot()?;

    let size = size()?;
    DIMENSIONS.set(size).unwrap(); // if DEVICE_SERIAL.set() is Ok, then this also
    if let Some(a) = app {
        let package = find_package(a)?;
        let activity = find_activity(&package)?;
        APP.set(package).unwrap();
        ACTIVITY.set(activity).unwrap();
    }
    Ok(())
}

fn connect(serial: &str) -> PyResult<()> {
    run(&["connect", serial]).map(|_| ())
}

fn wait_for_boot() -> PyResult<()> {
    while String::from_utf8_lossy(&device_action(&["shell", "getprop", "sys.boot_completed"])?.stdout).trim() != "1" {
        Python::attach(|py| py.check_signals())?;
        sleep(Duration::from_secs(1));
    }
    Ok(())
}

fn size() -> PyResult<Point> {
    let output = device_action(&["shell", "wm", "size"])?.stdout;
    let size_str = String::from_utf8_lossy(&output);
    parse_size(&size_str)
}

fn parse_size(size_output: &str) -> PyResult<Point> {
    let size_part = size_output
        .split_whitespace()
        .last()
        .ok_or_else(|| PyConnectionError::new_err("adb is not ready. retry"))?;
    let (width, height) = size_part.split_once('x').ok_or_else(|| {
        PyValueError::new_err(format!("Failed to get size from output: {size_output}"))
    })?;
    Ok([width.parse()?, height.parse()?])
}

fn find_package(name: String) -> PyResult<String> {
    let output = device_action(&["shell", "pm", "list", "packages"])?;
    let packages = String::from_utf8_lossy(&output.stdout);

    packages
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .find(|&package| package.contains(&name))
        .map(String::from)
        .ok_or_else(|| {
            PyValueError::new_err(format!("Package '{}' not found", name))
        })
}

fn find_activity(package: &str) -> PyResult<String> {
    let output = device_action(&[
        "shell",
        "cmd",
        "package",
        "resolve-activity",
        "--brief",
        package,
    ])?;

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .nth(1)
        .filter(|line| line.contains('/'))
        .map(|line| line.trim().to_string())
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "Activity not found for '{}'",
                package
            ))
        })
}

#[pyfunction]
pub(super) fn start_app() -> PyResult<()> {
    let activity = ACTIVITY.get().ok_or_else(|| {
        PyValueError::new_err(
            "App name not set. Call device_config with app argument before start_app().",
        )
    })?;
    device_action(&["shell", "am", "start", "-n", activity]).map(|_| ())
}

#[pyfunction]
pub(super) fn close_app() -> PyResult<()> {
    let name = APP.get().ok_or_else(|| {
        PyValueError::new_err(
            "App name not set. Call device_config with app argument before close_app().",
        )
    })?;
    device_action(&["shell", "am", "force-stop", name]).map(|_| ())
}

pub(super) fn tap(point: Point) -> PyResult<()> {
    device_action(&[
        "shell",
        "input",
        "tap",
        &point[0].to_string(),
        &point[1].to_string(),
    ])
    .map(|_| ())
}

pub(super) fn swipe(start: Point, end: Point, time: u16) -> PyResult<()> {
    device_action(&[
        "shell",
        "input",
        "swipe",
        &start[0].to_string(),
        &start[1].to_string(),
        &end[0].to_string(),
        &end[1].to_string(),
        &time.to_string(),
    ])
    .map(|_| ())
}

pub(crate) fn screencap() -> PyResult<(u32, u32, Vec<u8>)> {
    let mut output = device_action(&["exec-out", "screencap"])?.stdout;
    if output.is_empty() {
        return Err(PyConnectionAbortedError::new_err(
            "adb screencap error. probably keyboard interrupt or device disconnected.",
        ));
    }
    let [width, height]: [u32; 2] = output
        .drain(..16)
        .array_chunks::<4>()
        .map(u32::from_le_bytes)
        .take(2)
        .collect::<Vec<u32>>()
        .try_into()
        .map_err(|v| PyBufferError::new_err(format!("Expected: width, height, found {:?}", v)))?;

    if output.len() != usize::try_from(width * height)? * RGBA_CHANNELS  {
        return Err(PyBufferError::new_err("adb screencap buffer error. probably keyboard interrupts or device disconnected."));
    }

    Ok((width, height, output))
}

pub(crate) fn back() -> PyResult<()> {
    device_action(&["shell", "input", "keyevent", "4"]).map(|_| ())
}

pub(crate) fn home() -> PyResult<()> {
    device_action(&["shell", "input", "keyevent", "3"]).map(|_| ())
}

pub(crate) fn write(text: &str) -> PyResult<()> {
    let text = text.replace(' ', "%s");
    device_action(&["shell", "input", "text", &text]).map(|_| ())
}

fn scan() -> PyResult<Vec<String>> {
    let raw_output = run(&["devices"])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output);

    let devices = text_output.lines().skip(1);
    Ok(devices
        .filter_map(|line| {
            let (serial, status) = line.split_once('\t')?;
            (status == "device").then(|| serial.to_owned())
        })
        .collect())
}

fn device_action(args: &[&str]) -> PyResult<Output> {
    let serial = DEVICE_SERIAL.get().ok_or_else(|| {
        PyValueError::new_err("serial not set. call device_config() before using actions.")
    })?;
    let args = [&["-s", serial], args].concat();
    run(&args)
}

fn run(args: &[&str]) -> PyResult<Output> {
    Command::new(ADB.get().unwrap())
        .args(args)
        .output()
        .map_err(|e| PyOSError::new_err(format!("ADB Error.\n{e}")))
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn parses_the_last_reported_screen_size() {
        assert_eq!(
            parse_size("Physical size: 1080x2400\nOverride size: 720x1600\n").unwrap(),
            [720, 1600]
        );
    }

    #[test]
    fn rejects_malformed_screen_sizes() {
        assert!(parse_size("Physical size: unknown").is_err());
        assert!(parse_size("Physical size: 1080xnot-a-number").is_err());
        assert!(parse_size("").is_err());
    }
}
