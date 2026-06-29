use std::fs::File;
use std::io::{Write, stdin};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;
use pyo3::exceptions::{PyBufferError, PyOSError, PyRuntimeError};
use pyo3::prelude::{PyResult, pyfunction};

const ADB_PORT_LENGTH: usize = 5;

static ADB: OnceLock<PathBuf> = OnceLock::new();
static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();

pub(super) fn device_config(adb: PathBuf, ip: Option<String>) -> PyResult<()> {
    println!("connecting adb device...");
    ADB.set(adb)
        .map_err(|_| PyRuntimeError::new_err("device_config function can only be called once."))?;

    let mut serial: Option<String> = scan()?;

    if let Some(ip) = ip {
        while serial.is_none() {
            let port = input_port()?;
            if let Some(port) = port {
                connect(&format!("{}:{}", ip, port))?;
            }

            serial = scan()?;
        }
    } else {
        while serial.is_none() {
            serial = scan()?;
        }
    }

    DEVICE_SERIAL.set(serial.unwrap())
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    println!("device connected");
    Ok(())
}

pub(super) fn tap(coords: [u16; 2]) -> PyResult<()> {
    device_action(&[
        "shell",
        "input",
        "tap",
        &coords[0].to_string(),
        &coords[1].to_string(),
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
    let [width, height]: [u32; 2] = output.drain(..16)
        .array_chunks::<4>()
        .map(|chunk| u32::from_le_bytes(chunk))
        .take(2)
        .collect::<Vec<u32>>()
        .try_into()
        .map_err(|v| PyBufferError::new_err(format!("Expected: width, height, found {:?}", v)))
        ?;

    Ok((width, height, output))
}


pub(crate) fn back() -> PyResult<()> {
    device_action(&["shell", "input", "keyevent", "4"]).map(|_| ())
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

    stdin().read_line(&mut input)
        .map_err(|e| PyOSError::new_err(e.to_string()))?;

    let port = input.trim();

    Ok(
        if port.parse::<u32>().is_ok() && port.len() == ADB_PORT_LENGTH {
            Some(port.to_string())
        } else {
            None
        }
    )
}

fn connect(port: &str) -> PyResult<bool> {
    let raw_output = run(&["connect", port])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    Ok(text_output.contains("connected to"))
}

fn device_action(args: &[&str]) -> PyResult<Output> {
    let serial = DEVICE_SERIAL
        .get()
        .map_err(|_| PyValueError::new_err("serial not set. call device_config() before using actions."))?;
    let args = [&["-s", serial], args].concat();
    run(&args)
}

fn run(args: &[&str]) -> PyResult<Output> {
    Command::new(ADB.get().unwrap())
        .args(args)
        .output()
        .map_err(|e| PyOSError::new_err(e.to_string()))
}
