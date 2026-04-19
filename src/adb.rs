use std::process::{Command, Output};
use std::io::stdin;
use std::fs;
use std::sync::OnceLock;


use crate::{paths, Coords};


const ADB_PORT_LENGTH: usize = 5;


static DEVICE_SERIAL: OnceLock<String> = OnceLock::new();



pub(super) fn device_config() {
    println!("connecting adb device...");

    let ip = get_ip();
    let mut serial: Option<String> = scan();


    while scan().is_none() {

        let port = input_port();
        if let Some(port) = port {
            connect(&format!("{}:{}", ip, port));
        }

        serial = scan();
    }
    DEVICE_SERIAL.set(serial.unwrap()).unwrap();
    println!("device connected");
}


pub(super) fn tap(coords: &Coords) {
    device_action(&["shell", "input", "tap", &coords.x.to_string(), &coords.y.to_string()]);
}


pub(crate) fn screencap() -> Vec<u8> {
    let mut v = device_action(&["exec-out", "screencap"]).stdout;
    v.drain(..16);
    v
}


pub(crate) fn dimensions() -> (u32, u32) {
    let output = device_action(&["shell", "wm", "size"]).stdout;
    let size_str = String::from_utf8_lossy(&output);

    let size_part = size_str
        .split_whitespace()
        .last()
        .unwrap();

    let size = size_part.split('x')
        .map(|s| s.parse::<u32>().unwrap())
        .collect::<Vec<u32>>();

    (size[0], size[1])
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
    println!("enter device port: ");
    let mut input = String::new();

    stdin()
        .read_line(&mut input)
        .expect("Failed to read line");

    let port = input.trim();


    return if port.parse::<u8>().is_ok() && port.len() == ADB_PORT_LENGTH {
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
    let serial = DEVICE_SERIAL.get().unwrap();
    let args = [&["-s", serial], args].concat();
    run(&args)
}


fn run(args: &[&str]) -> std::process::Output {
    Command::new("adb")
        .args(args)
        .output()
        .expect("Failed to execute adb command")
}


fn get_ip() -> String {
    let ip_path = paths::ip();
    let raw_ip = fs::read_to_string(ip_path).expect("Failed to read device IP file");
    raw_ip.trim().to_string()
}
