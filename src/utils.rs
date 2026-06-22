use pixen::Image;
use std::{fs, io};
use std::collections::{HashMap, HashSet};
use std::sync::RwLockWriteGuard;

use pyo3::prelude::PyResult;
use pyo3::exceptions::PyRuntimeError;

use crate::{DATA, DATA_PATH};

pub(crate) fn rgba_into_rgb(rgba: Vec<u8>) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);

    for [r, g, b, _] in rgba.into_chunks::<4>() {
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    rgb
}

pub(super) fn add_coords(key: &str, val: [u16; 2]) -> PyResult<()> {
    get_lock()?.get_mut(key).unwrap().0.insert(val);
    save_data()?;
    Ok(())
}

fn get_lock() -> PyResult<RwLockWriteGuard<'static, HashMap<String, (HashSet<[u16; 2]>, u8)>>> {
    DATA
        .write()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

pub(super) fn set_tolerance(key: &str, tolerance: u8) -> PyResult<()> {
    let mut lock = get_lock()?;
    let t = lock.get_mut(key).unwrap();
    t.1 = t.1.max(tolerance);
    drop(t);
    save_data()?;
    Ok(())
}

pub(super) fn reset_tolerance(key: &str) -> PyResult<()> {
    get_lock()?.get_mut(key).unwrap().1 = 0;
    Ok(())
}

fn save_data() -> PyResult<()> {
    let writer = io::BufWriter::new(fs::File::create(DATA_PATH.get().unwrap()).map_err(|e| PyRuntimeError::new_err(e.to_string()))?);
    serde_json::to_writer_pretty(
        writer,
        &*DATA.read()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
    )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(())
}

pub(super) fn center_coords(corner: [u16; 2], img: &Image) -> [u16; 2] {
    [
        corner[0] + img.width() as u16 / 2,
        corner[1] + img.height() as u16 / 2,
    ]
}
