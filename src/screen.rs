use std::fs::File;
use std::sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use pixen::Image;
use png::{BitDepth, ColorType, Encoder};
use pyo3::exceptions::{PyOSError, PyRuntimeError};
use pyo3::prelude::PyResult;

use crate::adb;
use crate::adb::DEVICE_SERIAL;
use crate::utils::rgba_into_rgb;

pub(crate) const RGB_CHANNELS: usize = 3;
pub(crate) const RGBA_CHANNELS: usize = 4;

static SCREENSHOT: RwLock<Option<Image>> = RwLock::new(None);

/// Returns the cached screen image, capturing and caching a new screenshot when needed.
///
/// Call [`reset`] after an action that changes the device screen so the next call captures a
/// fresh image.
pub(super) fn get() -> PyResult<MappedRwLockReadGuard<'static, Image>> {
    let mut guard = SCREENSHOT.write().unwrap();

    if guard.is_none() {
        let (w, h, rgba_bytes) = adb::screencap()?;

        let image = Image::new(
            rgba_into_rgb(rgba_bytes),
            w.try_into().unwrap(),
            h.try_into().unwrap(),
            RGB_CHANNELS.try_into().unwrap(),
        )
        .map_err(|e| PyRuntimeError::new_err(e))?;

        *guard = Some(image);
    }
    drop(guard);
    Ok(RwLockReadGuard::map(SCREENSHOT.read().unwrap(), |img| {
        img.as_ref().unwrap()
    }))
}

pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}

pub(crate) fn save() -> PyResult<()> {
    let guard = get()?;
    let buffer = guard.as_raw();
    let [width, height] = guard.dimensions();

    let file = File::create(filename()?)?;
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

pub(crate) fn filename() -> PyResult<String> {
    let device = DEVICE_SERIAL.get().ok_or_else(|| {
        PyRuntimeError::new_err("device_config should be called before taking a screenshot.")
    })?;
    let safe_device = device.replace(':', "_");

    Ok(format!("screen-{safe_device}.png"))
}
