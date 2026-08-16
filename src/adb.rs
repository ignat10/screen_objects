use pyo3::exceptions::{PyBufferError, PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::{PyResult, Python, pyfunction};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;

use crate::Coords;

static ADB: OnceLock<PathBuf> = OnceLock::new();
pub(crate) static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();
static APP: OnceLock<String> = OnceLock::new();
pub(crate) static DIMENSIONS: OnceLock<Coords> = OnceLock::new();

#[pyfunction]
#[pyo3(signature = (adb = PathBuf::from("adb"), serial = None, app = None))]
pub(super) fn device_config(
    adb: PathBuf,
    serial: Option<String>,
    app: Option<String>,
) -> PyResult<()> {
    println!("connecting adb device...");
    ADB.set(adb)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;

    let device;
    if let Some(serial) = serial {
        loop {
            Python::attach(|py| py.check_signals())?;
            if scan()?.contains(&serial) {
                device = serial;
                break;
            }
        }
    } else {
        loop {
            if let Some(serial) = scan()?.first() {
                device = serial.to_string();
                break;
            }
        }
    }

    DEVICE_SERIAL
        .set(device)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;
    println!("device connected");

    let size = size()?;
    DIMENSIONS
        .set(size)
        .unwrap(); // if DEVICE_SERIAL.set() is Ok, then this also
    if let Some(a) = app {
        let package = find_package(a)?;
        APP.set(package).unwrap();
    }
    Ok(())
}

fn size() -> PyResult<Coords> {
    let output = device_action(&["shell", "wm", "size"])?.stdout;
    let size_str = String::from_utf8_lossy(&output);

    let size_part = size_str.split_whitespace().last().unwrap();

    size_part
        .split('x')
        .map(|s| s.parse::<u16>().unwrap())
        .collect::<Vec<u16>>()
        .try_into()
        .map_err(|_| PyValueError::new_err(format!("Failed to get size from output: {}", size_str)))
}

fn find_package(name: String) -> PyResult<String> {
    let output = device_action(&["shell", "pm", "list", "packages"])?;
    let packages = String::from_utf8_lossy(&output.stdout);
    for line in packages.lines() {
        let package = line
            .strip_prefix("package:")
            .ok_or_else(|| PyBufferError::new_err("line is not starting with package: "))?;
        if package.contains(&name) {
            return Ok(package.into());
        }
    }
    Err(PyValueError::new_err(format!(
        "Package '{}' not found",
        name
    )))
}

#[pyfunction]
pub(super) fn start_app() -> PyResult<()> {
    let name = APP.get().ok_or_else(|| {
        PyValueError::new_err(
            "App name not set. Call device_config with app argument before this function.",
        )
    })?;
    device_action(&["shell", "monkey", "-p", name, "1"]).map(|_| ())
}

#[pyfunction]
pub(super) fn close_app() -> PyResult<()> {
    let name = APP.get().ok_or_else(|| {
        PyValueError::new_err(
            "App name not set. Call device_config with app argument before this function.",
        )
    })?;
    device_action(&["shell", "am", "force-stop", name]).map(|_| ())
}

pub(super) fn tap(coords: Coords) -> PyResult<()> {
    device_action(&[
        "shell",
        "input",
        "tap",
        &coords[0].to_string(),
        &coords[1].to_string(),
    ])
    .map(|_| ())
}

pub(super) fn swipe(start: Coords, end: Coords, time: u16) -> PyResult<()> {
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
    let [width, height]: [u32; 2] = output
        .drain(..16)
        .array_chunks::<4>()
        .map(|chunk| u32::from_le_bytes(chunk))
        .take(2)
        .collect::<Vec<u32>>()
        .try_into()
        .map_err(|v| PyBufferError::new_err(format!("Expected: width, height, found {:?}", v)))?;

    Ok((width, height, output))
}

pub(crate) fn back() -> PyResult<()> {
    device_action(&["shell", "input", "keyevent", "4"]).map(|_| ())
}

pub(crate) fn home() -> PyResult<()> {
    device_action(&["shell", "input", "keyevent", "3"]).map(|_| ())
}

fn scan() -> PyResult<Vec<String>> {
    let raw_output = run(&["devices"])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    let devices = text_output.lines().skip(1);
    Ok(devices
        .filter_map(|line| {
            let [serial, status]: [&str; 2] = line
                .split_whitespace()
                .collect::<Vec<&str>>()
                .try_into()
                .unwrap();
            if status == "device" {
                Some(serial.to_string())
            } else {
                None
            }
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
