use std::path::PathBuf;
use std::{fs, io};

use fastrand::u16;
use pixen::{Image, Point};
use pyo3::Python;
use pyo3::exceptions::{PyBufferError, PyRuntimeError};
use pyo3::prelude::PyResult;
use stb_image::image;

use crate::{Coords, OBJECTS_DATA, OBJECTS_PATH, REGIONS_DATA, REGIONS_PATH};

pub(crate) const RGB_CHANNELS: u16 = 3;
pub(crate) const RGBA_CHANNELS: u16 = 4;

pub(crate) fn rgba_into_rgb(rgba: Vec<u8>) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);

    for [r, g, b, _] in rgba.into_iter().array_chunks::<4>() {
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    rgb
}

pub(super) fn save_objects() -> PyResult<()> {
    save(OBJECTS_PATH.get().unwrap(), &*OBJECTS_DATA.read().unwrap())
}

pub(super) fn save_regions() -> PyResult<()> {
    save(REGIONS_PATH.get().unwrap(), &*REGIONS_DATA.read().unwrap())
}

fn save(path: &PathBuf, data: impl serde::Serialize) -> PyResult<()> {
    let writer = io::BufWriter::new(
        fs::File::create(path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?,
    );
    serde_json::to_writer_pretty(writer, &data).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

pub(super) fn load_image(path: &PathBuf) -> PyResult<Image> {
    match image::load(&path) {
        image::LoadResult::ImageU8(img) => Ok(Image::new(
            match img.depth.try_into().unwrap() {
                RGB_CHANNELS => img.data,
                RGBA_CHANNELS => rgba_into_rgb(img.data),
                c => {
                    return Err(PyBufferError::new_err(format!(
                        "unknown number of channels: {}",
                        c
                    )));
                }
            },
            img.width.try_into()?,
            img.height.try_into()?,
            RGB_CHANNELS.try_into()?,
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?),
        image::LoadResult::ImageF32(_) => Err(PyBufferError::new_err(format!(
            "Unknown image format: {}",
            path.display()
        ))),
        image::LoadResult::Error(e) => Err(PyBufferError::new_err(format!(
            "Failed to load image: {}",
            e
        ))),
    }
}

pub(super) fn center_coords(top_left: Point, img: &Image) -> Coords {
    let w = img.width();
    let h = img.height();
    let w_quarter = w / 4;
    let h_quarter = h / 4;
    let rand_w = u16(w_quarter..w_quarter * 3);
    let rand_h = u16(h_quarter..h_quarter * 3);
    [top_left[0] + rand_w, top_left[1] + rand_h]
}

pub(crate) fn check_python_signals() -> PyResult<()> {
    Python::attach(|py| py.check_signals())
}
