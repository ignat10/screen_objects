#![feature(vec_into_chunks)]
#![feature(mapped_lock_guards)]
#![feature(iter_array_chunks)]
#![feature(pathbuf_into_string)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};

use pixen::Image;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::from_reader;
use stb_image::image;
use walkdir::WalkDir;

pub mod adb;
mod screen;
pub mod utils;

use utils::*;

const BASE_TOLERANCE: u8 = 5;

#[pymodule]
mod screen_objects {
    use super::*;

    #[pymodule_export]
    use get_objects;

    #[pymodule_export]
    use ScreenObject;

    #[pymodule_export]
    use adb::screenshot;

    #[pyfunction]
    fn reset_screen() {
        screen::reset();
    }

    #[pyfunction]
    fn device_config(adb: PathBuf, ip: Option<String>) -> PyResult<()> {
        adb::device_config(adb, ip)
    }

    #[pyfunction]
    fn back() -> PyResult<()> {
        adb::back()?;
        screen::reset();
        Ok(())
    }
}

static DATA_PATH: OnceLock<PathBuf> = OnceLock::new();
static DATA: LazyLock<RwLock<HashMap<String, (Option<[u16; 2]>, u8)>>> = LazyLock::new(|| {
    let path = DATA_PATH.get().expect("DATA_PATH must be initialized");

    if !path.exists() {
        fs::write(&path, "{}")
            .expect(format!("Failed to write empty file {}", path.to_str().unwrap()).as_str());
    }

    let file = fs::File::open(&path).expect(format!("Failed to open file {}", path.display()).as_str());

    let map = from_reader(file).expect(format!("Failed to parse JSON {}", path.display()).as_str());

    RwLock::new(map)
});

#[pyfunction]
fn get_objects(samples_dir: PathBuf) -> PyResult<HashMap<String, ScreenObject>> {
    let files = WalkDir::new(&samples_dir)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path());

    DATA_PATH
        .set(
            samples_dir
                .parent()
                .unwrap()
                .to_path_buf()
                .join("objects_data.json"),
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    let mut lock = DATA
        .write()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let mut objects = HashMap::new();
    for file in files {
        let name = file
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        if !lock.contains_key(&name) {
            lock.insert(name.clone(), (None, BASE_TOLERANCE));
        };
        let (coords, tolerance) = lock.get(&name).unwrap().clone();

        objects.insert(name, ScreenObject::new(file, coords, tolerance));
    }
    Ok(objects)
}

#[pyclass]
struct ScreenObject {
    name: String,
    image: LazyLock<Image, Box<dyn FnOnce() -> Image + Send + Sync>>,
    coords: Option<[u16; 2]>,
    tolerance: u8,
}

#[pymethods]
impl ScreenObject {
    fn config(&self, fixed: bool) -> PyResult<()> {
        let screenshot = screen::get()?;
        let image = &self.image;

        let (diff, coords) = pixen::get_tolerance(&*screenshot, image);

        *DATA.write().unwrap().get_mut(&self.name).unwrap() = (
            if fixed {
                Some(coords)
            } else {
                None
            },
            diff + 1
        );
        save_data()?;
        Ok(())
    }

    fn exists(&self) -> PyResult<bool> {
        self.is_on_screen()
    }

    fn tap(&self) -> PyResult<bool> {
        if let Some(coords) = self.find_object()? {
            let center = center_coords(coords, &self.image);
            Python::attach(|py| py.check_signals())?;
            adb::tap(center)?;
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn tap_nth(&self, n: usize) -> PyResult<bool> {
        if let Some(coords) = self.find_nth(n)? {
            let center = center_coords(coords, &self.image);
            Python::attach(|py| py.check_signals())?;
            adb::tap(center)?;
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn count(&self) -> PyResult<u16> {
        let screenshot = screen::get()?;

        Ok(pixen::count(&*screenshot, &self.image, self.tolerance))
    }

    fn spam_tap(&self, n: u8, interval: f32) -> PyResult<bool> {
        if let Some(coords) = self.find_object()? {
            let image = &self.image;
            let center = center_coords(coords, image);
            for _ in 0..n {
                adb::tap(center)?;
                std::thread::sleep(std::time::Duration::from_secs_f32(interval));
                Python::attach(|py| py.check_signals())?;
            }
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl ScreenObject {
    fn new(path: PathBuf, coords: Option<[u16; 2]>, tolerance: u8) -> Self {
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        Self {
            name,
            image: LazyLock::new(Box::new(move || {
                let img = image::load(&path);
                match img {
                    image::LoadResult::ImageU8(img) => Image::new(
                        match img.depth {
                            screen::RGB_CHANNELS => img.data,
                            screen::RGBA_CHANNELS => rgba_into_rgb(img.data),
                            c => panic!("unknown number of channels: {}", c),
                        },
                        img.width,
                        img.height,
                        screen::RGB_CHANNELS,
                    ).unwrap(),
                    image::LoadResult::ImageF32(_) => {
                        panic!("Unknown image format: {}", path.display())
                    }
                    image::LoadResult::Error(e) => panic!("Failed to load image: {}", e),
                }
            })),
            coords,
            tolerance,
        }
    }

    fn find_object(&self) -> PyResult<Option<[u16; 2]>> {
        if self.coords.is_some() {
            if self.matches_at_coords()? {
                Ok(self.coords)
            } else {
                Ok(None)
            }
        } else {
            let screenshot = screen::get()?;
            let tolerance = self.tolerance;
            Ok(pixen::find_best(&screenshot, &self.image, tolerance))
        }
    }

    fn is_on_screen(&self) -> PyResult<bool> {
        if self.coords.is_some() {
            Ok(self.matches_at_coords()?)
        } else {
            let screenshot = screen::get()?;
            let image = &self.image;
            let tolerance = self.tolerance;

            Ok(pixen::matches(&*screenshot, image, tolerance))
        }
    }

    fn find_nth(&self, n: usize) -> PyResult<Option<[u16; 2]>> {
        if self.coords.is_some() {
            Err(PyValueError::new_err(format!("Tried to find nth {}, but it has fixed coords.", self.name)))
        } else {
            let screenshot = screen::get()?;
            let image = &self.image;
            let tolerance = self.tolerance;

            Ok(pixen::find_nth(&*screenshot, image, tolerance, n))
        }
    }

    fn matches_at_coords(&self) -> PyResult<bool> {
        let coords = self.coords.ok_or(PyValueError::new_err(format!("Not found coords for {}.", self.name)))?;
        let screenshot = screen::get()?;
        let image = &self.image;
        let tolerance = self.tolerance;

        Ok(pixen::matches_at(&*screenshot, image, coords, tolerance))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::{Value, json, to_string_pretty};
    use tempfile::{TempDir, tempdir};

    static SAMPLES: LazyLock<TempDir> = LazyLock::new(|| tempdir().unwrap());
    static DATA: LazyLock<Value> = LazyLock::new(|| {
        json!({
            "alpha": (
                [100, 200],
                5
            ),
            "delta": (
                [1, 1],
                6
            )
        })
    });
    static OBJECTS: LazyLock<HashMap<String, ScreenObject>> = LazyLock::new(|| {
        for obj in DATA.as_object().unwrap().keys() {
            let path = SAMPLES.path().join(format!("{}.png", obj));
            fs::File::create(path).unwrap();
        }
        fs::write(
            SAMPLES.path().parent().unwrap().join("objects_data.json"),
            to_string_pretty(&*DATA).unwrap(),
        )
        .unwrap();
        get_objects(SAMPLES.path().to_path_buf()).unwrap()
    });

    #[test]
    fn get_objects_with_data() {
        let obj = OBJECTS.get("alpha").unwrap();
        let coords = obj.coords.clone();

        assert_eq!(coords, Some([100, 200]));
    }

    #[test]
    fn get_objects_without_data() {
        let obj = OBJECTS.get("delta").unwrap();
        assert_eq!(obj.coords, Some([1, 1]));
    }
}
