use pixen::{Image, Point};
use std::{fs, io};

use pyo3::prelude::PyResult;
use pyo3::exceptions::PyRuntimeError;

use crate::{DATA, DATA_PATH};
use crate::screen::RGBA_CHANNELS;

pub(crate) fn rgba_into_rgb(rgba: Vec<u8>) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);

    for [r, g, b, _] in rgba.into_iter().array_chunks::<RGBA_CHANNELS>() {
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    rgb
}

pub(super) fn save_data() -> PyResult<()> {
    let writer = io::BufWriter::new(fs::File::create(DATA_PATH.get().unwrap())
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?);
    serde_json::to_writer_pretty(
        writer,
        &*DATA.read()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
    )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(())
}

pub(super) fn center_coords(corner: Point, img: &Image) -> Point {
    [
        corner[0] + img.width() / 2,
        corner[1] + img.height() / 2,
    ]
}
