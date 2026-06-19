use std::fs::File;
use std::io::{Write, stdin};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{LazyLock, OnceLock};

use pyo3::prelude::{PyResult, pyfunction};

const ADB_PORT_LENGTH: usize = 5;

static ADB: OnceLock<PathBuf> = OnceLock::new();
static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();

pub(super) fn device_config(adb: PathBuf, ip: Option<String>) {
    println!("connecting adb device...");
    ADB.set(adb).unwrap();

    let mut serial: Option<String> = scan();

    if let Some(ip) = ip {
        while serial.is_none() {
            let port = input_port();
            if let Some(port) = port {
                connect(&format!("{}:{}", ip, port));
            }

            serial = scan();
        }
    } else {
        while serial.is_none() {
            serial = scan();
        }
    }

    DEVICE_SERIAL.set(serial.unwrap()).unwrap();
    println!("device connected");
}

pub(crate) static DIMENTIONS: LazyLock<[u16; 2]> = LazyLock::new(|| {
    let output = device_action(&["shell", "wm", "size"]).stdout;
    let size_str = String::from_utf8_lossy(&output);

    let size_part = size_str.split_whitespace().last().unwrap();

    size_part
        .split('x')
        .map(|s| s.parse::<u16>().unwrap())
        .collect::<Vec<u16>>()
        .try_into()
        .unwrap()
});

pub(super) fn tap(coords: [u16; 2]) {
    device_action(&[
        "shell",
        "input",
        "tap",
        &coords[0].to_string(),
        &coords[1].to_string(),
    ]);
}

#[pyfunction]
pub(super) fn screenshot() -> PyResult<()> {
    let mut file = File::create("screen.png")?;
    let out = device_action(&["exec-out", "screencap", "-p"]).stdout;
    file.write_all(&out)?;
    Ok(())
}

pub(crate) fn screencap() -> Vec<u8> {
    let mut v = device_action(&["exec-out", "screencap"]).stdout;
    v.drain(..16);
    v
}

pub(crate) fn back() {
    device_action(&["shell", "input", "keyevent", "4"]);
}

fn scan() -> Option<String> {
    let raw_output = run(&["devices"]).stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    for line in text_output.lines().skip(1) {
        if line.is_empty() {
            return None;
        }

        let mut serial_status = line.split_whitespace();

        let serial = serial_status.next().unwrap();
        let status = serial_status.next().unwrap();

        if status == "device" {
            return Some(serial.to_string());
        }
    }

    None
}

fn input_port() -> Option<String> {
    println!("Turn on USB debugging or enter wireless debugging port: ");
    let mut input = String::new();

    stdin().read_line(&mut input).expect("Failed to read line");

    let port = input.trim();

    if port.parse::<u32>().is_ok() && port.len() == ADB_PORT_LENGTH {
        Some(port.to_string())
    } else {
        None
    }
}

fn connect(port: &str) -> bool {
    let raw_output = run(&["connect", port]).stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    text_output.contains("connected to")
}

fn device_action(args: &[&str]) -> Output {
    let serial = DEVICE_SERIAL
        .get()
        .expect("serial not set. call device_config() before using actions.");
    let args = [&["-s", serial], args].concat();
    run(&args)
}

fn run(args: &[&str]) -> Output {
    Command::new(ADB.get().unwrap())
        .args(args)
        .output()
        .expect("Failed to execute adb command")
}
