use crate::utils::{RGB_CHANNELS, rgba_into_rgb};
use crate::{
    Coords, Direction, MINUTE, SwipeSpeed, check_python_signals, screen_object, screen_region,
};
use pixen::Image;
use png::{BitDepth, ColorType, Encoder};
use pyo3::exceptions::{PyBufferError, PyException, PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::{Py, PyRef, PyResult, Python, pyfunction};
use pyo3::{PyErr, pyclass, pymethods};
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ADB: OnceLock<PathBuf> = OnceLock::new();

#[pyfunction]
#[pyo3(signature = (adb = PathBuf::from("adb"), app = None))]
pub(super) fn get_devices(adb: PathBuf, app: Option<String>) -> PyResult<Vec<Device>> {
    ADB.set(adb)
        .map_err(|_| PyRuntimeError::new_err("get_devices can only be called once"))?;

    scan()?
        .into_iter()
        .map(|serial| Device::connect(serial, app.clone()))
        .collect()
}

#[pyclass]
pub(super) struct Device {
    serial: String,
    dimensions: Coords,
    screen: Option<Image>,
    app: Option<String>,
    #[pyo3(get, set)]
    is_available: bool,
}

#[pymethods]
impl Device {
    fn __getitem__(slf: PyRef<'_, Self>, name: &str) -> PyResult<DeviceObject> {
        screen_object(name)?;
        Ok(DeviceObject {
            device: slf.into(),
            name: name.to_owned(),
        })
    }

    fn calibrate(
        &mut self,
        name: &str,
        fixed: bool,
        region: Option<String>,
        n: Option<usize>,
    ) -> PyResult<()> {
        let screen = self.screencap()?;
        screen_object(name)?.calibrate(screen, fixed, region, n)
    }

    fn calibrate_region(&mut self, name: &str) -> PyResult<()> {
        let screen = self.screencap()?;
        screen_region(name)?.calibrate(screen)
    }

    fn exists(&mut self, name: &str) -> PyResult<bool> {
        let screen = self.screencap()?;
        screen_object(name)?.is_on_screen(screen)
    }

    #[pyo3(signature = (name, timeout = MINUTE))]
    fn wait(&mut self, name: &str, timeout: f32) -> PyResult<bool> {
        let timeout = Duration::from_secs_f32(timeout);
        let start = Instant::now();
        while !self.exists(name)? {
            if start.elapsed() > timeout {
                return Ok(false);
            }
            check_python_signals()?;
            self.reset_screen();
        }
        Ok(true)
    }

    #[pyo3(signature = (name, timeout = MINUTE))]
    fn force_wait(&mut self, name: &str, timeout: f32) -> PyResult<()> {
        if !self.wait(name, timeout)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    fn tap(&mut self, name: &str) -> PyResult<bool> {
        let screen = self.screencap()?;
        if let Some(coords) = screen_object(name)?.find_object(screen)? {
            check_python_signals()?;
            self.tap_at(coords)?;
            self.reset_screen();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_tap(&mut self, name: &str) -> PyResult<()> {
        if !self.tap(name)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    fn spam_tap(&mut self, name: &str, n: u8, interval: f32) -> PyResult<bool> {
        let screen = self.screencap()?;
        if let Some(coords) = screen_object(name)?.find_object(screen)? {
            for _ in 0..n {
                self.tap_at(coords)?;
                std::thread::sleep(Duration::from_secs_f32(interval));
                check_python_signals()?;
            }
            self.reset_screen();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_spam_tap(&mut self, name: &str, n: u8, interval: f32) -> PyResult<()> {
        if !self.spam_tap(name, n, interval)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    #[pyo3(signature = (name, timeout = MINUTE))]
    fn waitap(&mut self, name: &str, timeout: f32) -> PyResult<bool> {
        let timeout = Duration::from_secs_f32(timeout);
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.tap(name)? {
                return Ok(true);
            }
            check_python_signals()?;
            self.reset_screen();
        }
        Ok(false)
    }

    #[pyo3(signature = (name, timeout = MINUTE))]
    fn force_waitap(&mut self, name: &str, timeout: f32) -> PyResult<()> {
        if !self.waitap(name, timeout)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    fn swipe(
        &mut self,
        name: &str,
        dir: Direction,
        speed: SwipeSpeed,
        duration: f32,
    ) -> PyResult<bool> {
        let screen = self.screencap()?;
        if let Some(start) = screen_object(name)?.find_object(screen)? {
            let distance = (speed.pixels_per_second() * duration) as u16;
            let end = dir.destination(start, distance);
            let time = (duration * 1000.0) as u16;
            self.swipe_from(start, end, time)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_swipe(
        &mut self,
        name: &str,
        dir: Direction,
        speed: SwipeSpeed,
        duration: f32,
    ) -> PyResult<()> {
        if !self.swipe(name, dir, speed, duration)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    fn tap_nth(&mut self, name: &str, n: usize) -> PyResult<bool> {
        let screen = self.screencap()?;
        if let Some(coords) = screen_object(name)?.find_nth(screen, n)? {
            self.tap_at(coords)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_tap_nth(&mut self, name: &str, n: usize) -> PyResult<()> {
        if !self.tap_nth(name, n)? {
            return Err(self.force_error(name));
        }
        Ok(())
    }

    fn tap_each(&mut self, name: &str) -> PyResult<()> {
        let screen = self.screencap()?;
        for coords in screen_object(name)?.find_each(screen)? {
            self.tap_at(coords)?;
        }
        Ok(())
    }

    fn count(&mut self, name: &str) -> PyResult<usize> {
        let screen = self.screencap()?;
        screen_object(name)?.count_on_screen(screen)
    }

    fn tap_center(&mut self) -> PyResult<()> {
        let [w, h] = self.dimensions;
        self.tap_at([w / 2, h / 2])
    }

    fn swipe_center(&mut self, dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<()> {
        let [w, h] = self.dimensions;

        let start = [w / 2, h / 2];
        let distance = (speed.pixels_per_second() * duration) as u16;
        let end = dir.destination(start, distance);
        let time = (duration * 1000.0) as u16;
        self.reset_screen();
        self.swipe_from(start, end, time)
    }

    fn start_app(&self) -> PyResult<()> {
        let app = self.get_app()?;
        self.device_action(&["shell", "monkey", "-p", app, "1"])
            .map(|_| ())
    }

    fn close_app(&self) -> PyResult<()> {
        let app = self.get_app()?;
        self.device_action(&["shell", "am", "force-stop", app])
            .map(|_| ())
    }

    fn back(&self) -> PyResult<()> {
        self.device_action(&["shell", "input", "keyevent", "4"])
            .map(|_| ())
    }

    fn home(&self) -> PyResult<()> {
        self.device_action(&["shell", "input", "keyevent", "3"])
            .map(|_| ())
    }

    fn reset_screen(&mut self) {
        self.screen = None;
    }

    fn save_screen(&mut self) -> PyResult<()> {
        let screen = self.screencap()?;
        let [width, height] = screen.dimensions();
        let buffer = screen.as_raw();

        let file = File::create("screen.png")?;
        let mut encoder = Encoder::new(file, width.into(), height.into());
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);

        let mut writer = encoder
            .write_header()
            .map_err(|e| PyOSError::new_err(e.to_string()))?;

        writer
            .write_image_data(buffer)
            .map_err(|e| PyOSError::new_err(e.to_string()))
    }
}

/// A screen object bound to a specific device.
///
/// Python creates this proxy with `device["object_name"]`.
#[pyclass]
pub(super) struct DeviceObject {
    device: Py<Device>,
    name: String,
}

#[pymethods]
impl DeviceObject {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[pyo3(signature = (fixed = false, region = None, n = None))]
    fn calibrate(&self, fixed: bool, region: Option<String>, n: Option<usize>) -> PyResult<()> {
        self.with_device(|device| device.calibrate(&self.name, fixed, region, n))
    }

    fn exists(&self) -> PyResult<bool> {
        self.with_device(|device| device.exists(&self.name))
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn wait(&self, timeout: f32) -> PyResult<bool> {
        self.with_device(|device| device.wait(&self.name, timeout))
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn force_wait(&self, timeout: f32) -> PyResult<()> {
        self.with_device(|device| device.force_wait(&self.name, timeout))
    }

    fn tap(&self) -> PyResult<bool> {
        self.with_device(|device| device.tap(&self.name))
    }

    fn force_tap(&self) -> PyResult<()> {
        self.with_device(|device| device.force_tap(&self.name))
    }

    fn spam_tap(&self, n: u8, interval: f32) -> PyResult<bool> {
        self.with_device(|device| device.spam_tap(&self.name, n, interval))
    }

    fn force_spam_tap(&self, n: u8, interval: f32) -> PyResult<()> {
        self.with_device(|device| device.force_spam_tap(&self.name, n, interval))
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn waitap(&self, timeout: f32) -> PyResult<bool> {
        self.with_device(|device| device.waitap(&self.name, timeout))
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn force_waitap(&self, timeout: f32) -> PyResult<()> {
        self.with_device(|device| device.force_waitap(&self.name, timeout))
    }

    fn swipe(&self, dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<bool> {
        self.with_device(|device| device.swipe(&self.name, dir, speed, duration))
    }

    fn force_swipe(&self, dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<()> {
        self.with_device(|device| device.force_swipe(&self.name, dir, speed, duration))
    }

    fn tap_nth(&self, n: usize) -> PyResult<bool> {
        self.with_device(|device| device.tap_nth(&self.name, n))
    }

    fn force_tap_nth(&self, n: usize) -> PyResult<()> {
        self.with_device(|device| device.force_tap_nth(&self.name, n))
    }

    fn tap_each(&self) -> PyResult<()> {
        self.with_device(|device| device.tap_each(&self.name))
    }

    fn count(&self) -> PyResult<usize> {
        self.with_device(|device| device.count(&self.name))
    }

    fn __repr__(&self) -> String {
        format!("DeviceObject({:?})", self.name)
    }
}

impl DeviceObject {
    fn with_device<T>(&self, action: impl FnOnce(&mut Device) -> PyResult<T>) -> PyResult<T> {
        Python::attach(|py| {
            let mut device = self.device.try_borrow_mut(py)?;
            action(&mut device)
        })
    }
}

impl Device {
    fn connect(serial: String, app_name: Option<String>) -> PyResult<Self> {
        let output = run(&["-s", &serial, "shell", "wm", "size"])?.stdout;
        let size_str = String::from_utf8_lossy(&output);

        let size_part = size_str.split_whitespace().last().ok_or_else(|| {
            PyValueError::new_err(format!("Failed to get size from output: {size_str}"))
        })?;

        let dimensions = size_part
            .split('x')
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                PyValueError::new_err(format!("Failed to get size from output: {size_str}"))
            })?
            .try_into()
            .map_err(|_| {
                PyValueError::new_err(format!("Failed to get size from output: {}", size_str))
            })?;

        let app = app_name
            .map(|name| {
                String::from_utf8_lossy(
                    &run(&["-s", &serial, "shell", "pm", "list", "packages"])?.stdout,
                )
                .lines()
                .find_map(|line| {
                    let package = line.strip_prefix("package:")?;
                    if package.contains(&name) {
                        Some(package.to_string())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    PyException::new_err(format!("No package found with app_name: {}", name))
                })
            })
            .transpose()?;
        Ok(Self {
            serial,
            screen: None,
            dimensions,
            app,
            is_available: true,
        })
    }

    fn screencap(&mut self) -> PyResult<&Image> {
        if self.screen.is_none() {
            let mut output = self.device_action(&["exec-out", "screencap"])?.stdout;
            if output.len() < 16 {
                return Err(PyBufferError::new_err(format!(
                    "Invalid screencap: expected at least 16 bytes, got {}",
                    output.len()
                )));
            }
            let [width, height]: [u32; 2] = output
                .drain(..16)
                .array_chunks::<4>()
                .map(|chunk| u32::from_le_bytes(chunk))
                .take(2)
                .collect::<Vec<u32>>()
                .try_into()
                .map_err(|values| {
                    PyBufferError::new_err(format!("Invalid screencap dimensions: {values:?}"))
                })?;

            self.screen = Some(
                Image::new(
                    rgba_into_rgb(output),
                    width
                        .try_into()
                        .map_err(|_| PyBufferError::new_err("Screenshot width exceeds u16"))?,
                    height
                        .try_into()
                        .map_err(|_| PyBufferError::new_err("Screenshot height exceeds u16"))?,
                    RGB_CHANNELS,
                )
                .map_err(|e| PyRuntimeError::new_err(e))?,
            );
        }
        Ok(self.screen.as_ref().unwrap())
    }

    fn tap_at(&mut self, coords: Coords) -> PyResult<()> {
        self.reset_screen();
        self.device_action(&[
            "shell",
            "input",
            "tap",
            &coords[0].to_string(),
            &coords[1].to_string(),
        ])
        .map(|_| ())
    }

    fn swipe_from(&mut self, start: Coords, end: Coords, time: u16) -> PyResult<()> {
        self.reset_screen();
        self.device_action(&[
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

    fn device_action(&self, args: &[&str]) -> PyResult<Output> {
        let args = [&["-s", &self.serial], args].concat();
        run(&args)
    }

    fn get_app(&self) -> PyResult<&str> {
        self.app
            .as_deref()
            .ok_or_else(|| PyValueError::new_err("App not set. Call get_devices with app argument"))
    }

    fn force_error(&mut self, name: &str) -> PyErr {
        if let Err(e) = self.save_screen() {
            return e;
        }
        PyRuntimeError::new_err(format!(
            "Called force method on {name} object, but it was not found.\nCheck log screen.png"
        ))
    }
}

fn scan() -> PyResult<Vec<String>> {
    let raw_output = run(&["devices"])?.stdout;
    let text_output = String::from_utf8_lossy(&raw_output).into_owned();

    Ok(text_output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut serial_status = line.split_whitespace();
            let serial = serial_status.next()?;
            let status = serial_status.next()?;
            if status == "device" {
                Some(serial.to_string())
            } else {
                None
            }
        })
        .collect())
}

fn run(args: &[&str]) -> PyResult<Output> {
    Command::new(ADB.get().unwrap())
        .args(args)
        .output()
        .map_err(|e| PyOSError::new_err(format!("ADB Error.\n{e}")))
}
