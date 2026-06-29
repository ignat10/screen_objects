use pixen::{Image, Point};
use std::{fs, io};
use std::path::PathBuf;
use pyo3::prelude::PyResult;
use pyo3::exceptions::{PyBufferError, PyRuntimeError};
use stb_image::image;
use crate::{OBJECTS_DATA, OBJECTS_PATH, REGIONS_DATA, REGIONS_PATH};
use crate::screen::{RGB_CHANNELS, RGBA_CHANNELS};

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

pub(super) fn save_objects() -> PyResult<()> {
    save(OBJECTS_PATH.get().unwrap(), &*OBJECTS_DATA.read().unwrap())
}

pub(super) fn save_regions() -> PyResult<()> {
    save(REGIONS_PATH.get().unwrap(), &*REGIONS_DATA.read().unwrap())
}

fn save(path: &PathBuf, data: impl serde::Serialize) -> PyResult<()> {
    let writer = io::BufWriter::new(
        fs::File::create(path)
         .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
    );
    serde_json::to_writer_pretty(writer, &data)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

pub(super) fn load_image(path: &PathBuf) -> PyResult<Image> {
    match image::load(&path) {
        image::LoadResult::ImageU8(img) => Ok(
            Image::new(
                match img.depth.try_into().unwrap() {
                    RGB_CHANNELS => img.data,
                    RGBA_CHANNELS => rgba_into_rgb(img.data),
                    c => return Err(PyBufferError::new_err(format!("unknown number of channels: {}", c))),
                },
                img.width.try_into()?,
                img.height.try_into()?,
                RGB_CHANNELS.try_into()?, 
            )
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        ),
        image::LoadResult::ImageF32(_) => Err(PyBufferError::new_err(format!("Unknown image format: {}", path.display()))),
        image::LoadResult::Error(e) => Err(PyBufferError::new_err(format!("Failed to load image: {}", e))),
    }
}

pub(super) fn center_coords(corner: Point, img: &Image) -> Point {
    [
        corner[0] + img.width() / 2,
        corner[1] + img.height() / 2,
    ]
}
