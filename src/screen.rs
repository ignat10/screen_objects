use pixen::Image;
use std::sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use pyo3::prelude::PyResult;
use pyo3::exceptions::PyRuntimeError;

use crate::adb;
use crate::utils::rgba_into_rgb;

pub(crate) const RGB_CHANNELS: usize = 3;

static SCREENSHOT: RwLock<Option<Image>> = RwLock::new(None);

pub(super) fn get() -> PyResult<MappedRwLockReadGuard<'static, Image>> {
    let mut guard = SCREENSHOT.write().unwrap();

    if guard.is_none() {
        let (w, h, rgba_bytes) = adb::screencap()?;

        let image = Image::new(
            rgba_into_rgb(rgba_bytes),
            w.try_into().unwrap(),
            h.try_into().unwrap(),
            RGB_CHANNELS,
        )
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        *guard = Some(image);
    }
    drop(guard);
    Ok(RwLockReadGuard::map(SCREENSHOT.read().unwrap(), |img| img.as_ref().unwrap()))
}

pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}
