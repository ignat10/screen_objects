use pyo3::exceptions::{PyBufferError, PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::{pyfunction, PyResult, Python};
use std::fs::File;
use std::io::{stdin, Write};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;

use crate::Coords;

const ADB_PORT_LENGTH: usize = 5;

static ADB: OnceLock<PathBuf> = OnceLock::new();
static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();
static APP: OnceLock<String> = OnceLock::new();
pub(crate) static DIMENSIONS: OnceLock<Coords> = OnceLock::new();

#[pyfunction]
#[pyo3(signature = (adb = PathBuf::from("adb"), ip = None, app = None))]
pub(super) fn device_config(adb: PathBuf, ip: Option<String>, app: Option<String>) -> PyResult<()> {
    println!("connecting adb device...");
    ADB.set(adb)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;

    let mut serial = scan()?;

    while serial.is_none() {
        Python::attach(|py| py.check_signals())?;
        if let Some(ip) = ip.clone() {
            let port = input_port()?;
            if let Some(port) = port {
                connect(&format!("{}:{}", ip, port))?;
            }

            serial = scan()?;
        }
    }

    DEVICE_SERIAL
        .set(serial.unwrap())
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    println!("device connected");

    let size = size()?;
    DIMENSIONS
        .set(size)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))
        ?;
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
        let package = line.strip_prefix("package:")
            .ok_or_else(|| PyBufferError::new_err("line is not starting with package: "))
            ?;
        if package.contains(&name) {
            return Ok(package.into());
        }
    }
    Err(PyValueError::new_err(format!("Package '{}' not found", name)))
}

#[pyfunction]
pub(super) fn start_app() -> PyResult<()> {
    let name = APP.get()
        .ok_or_else(|| PyValueError::new_err("App name not set. Call device_config with app argument before this function."))
        ?;
    device_action(&["shell", "monkey", "-p", name, "1"])
        .map(|_| ())
}

#[pyfunction]
pub(super) fn close_app() -> PyResult<()> {
    let name = APP.get()
        .ok_or_else(|| PyValueError::new_err("App name not set. Call device_config with app argument before this function."))
        ?;
    device_action(&["shell", "am", "force-stop", name])
        .map(|_| ())
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

#[pyfunction]
pub(super) fn screenshot() -> PyResult<()> {
    let mut file = File::create("screen.png")?;
    let out = device_action(&["exec-out", "screencap", "-p"])?.stdout;
    file.write_all(&out)?;
    Ok(())
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

fn scan() -> PyResult<Option<String>> {
    let raw_output = run(&["devices"])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    for line in text_output.lines().skip(1) {
        if line.is_empty() {
            return Ok(None);
        }

        let mut serial_status = line.split_whitespace();

        let serial = serial_status.next().unwrap();
        let status = serial_status.next().unwrap();

        if status == "device" {
            return Ok(Some(serial.to_string()));
        }
    }

    Ok(None)
}

fn input_port() -> PyResult<Option<String>> {
    println!("Turn on USB debugging or enter wireless debugging port: ");
    let mut input = String::new();

    stdin()
        .read_line(&mut input)
        .map_err(|e| PyOSError::new_err(e.to_string()))?;

    let port = input.trim();

    Ok(
        if port.parse::<u32>().is_ok() && port.len() == ADB_PORT_LENGTH {
            Some(port.to_string())
        } else {
            None
        },
    )
}

fn connect(port: &str) -> PyResult<bool> {
    let raw_output = run(&["connect", port])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    Ok(text_output.contains("connected to"))
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
        .map_err(|e| PyOSError::new_err(e.to_string()))
}
