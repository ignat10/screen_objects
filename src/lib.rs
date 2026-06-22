#![feature(vec_into_chunks)]
#![feature(mapped_lock_guards)]
#![feature(iter_array_chunks)]
#![feature(pathbuf_into_string)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};

use pixen::Image;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde_json::from_reader;
use stb_image::image;
use walkdir::WalkDir;

pub mod adb;
mod screen;
pub mod utils;

use screen::RGB_CHANNELS;
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
static DATA: LazyLock<RwLock<HashMap<String, (HashSet<[u16; 2]>, u8)>>> = LazyLock::new(|| {
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
    let files: Vec<PathBuf> = WalkDir::new(&samples_dir)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect();

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
            .and_then(|n| n.to_str())
            .unwrap()
            .to_string();

        if !lock.contains_key(&name) {
            lock.insert(name.clone(), (HashSet::new(), BASE_TOLERANCE));
        };

        objects.insert(name, ScreenObject::new(file));
    }
    Ok(objects)
}

#[pyclass]
struct ScreenObject {
    name: String,
    image: LazyLock<Image, Box<dyn FnOnce() -> Image + Send + Sync>>,
    coords: HashSet<[u16; 2]>,
    tolerance: u8,
}

#[pymethods]
impl ScreenObject {
    fn exists(&self) -> PyResult<bool> {
        self.is_on_screen()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn tap(&self) -> PyResult<bool> {
        Ok(
            if let Some(coords) = self
                .find_object()
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            {
                let center = center_coords(coords, &self.image);
                Python::attach(|py| py.check_signals())?;
                adb::tap(center)?;
                screen::reset();
                true
            } else {
                false
            }
        )
    }

    fn tap_nth(&self, n: usize) -> PyResult<bool> {
        Ok(
            if let Some(coords) = self
                .find_nth(n)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            {
                let center = center_coords(coords, &self.image);
                Python::attach(|py| py.check_signals())?;
                adb::tap(center)?;
                screen::reset();
                true
            } else {
                false
            }
        )
    }

    fn spam_tap(&self, n: u8, interval: f32) -> PyResult<bool> {
        Ok(
            if let Some(coords) = self
                .find_object()
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            {
                let image = &self.image;
                let center = center_coords(coords, image);
                for _ in 0..n {
                    adb::tap(center)?;
                    std::thread::sleep(std::time::Duration::from_secs_f32(interval));
                    Python::attach(|py| py.check_signals())?;
                }
                screen::reset();
                true
            } else {
                false
            }
        )
    }

    fn tap_best(&self) -> PyResult<u8> {
        let screenshot = screen::get()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let image = &self.image;

        let (diff, coords) = pixen::get_tolerance(&*screenshot, image);
        add_coords(&self.name, coords)?;
        set_tolerance(&self.name, diff)?;
        
        adb::tap(coords)?;
        Ok(diff)
    }
    
    fn reset_tolerance(&self) -> PyResult<()> {
        reset_tolerance(self.name.as_str())
    }
}

impl ScreenObject {
    fn new(path: PathBuf) -> Self {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let (coords, tolerance) = DATA.read().unwrap().get(&name).unwrap().clone();
        Self {
            name,
            image: LazyLock::new(Box::new(move || {
                let img = image::load(&path);
                match img {
                    image::LoadResult::ImageU8(img) => Image::new(
                        if img.depth != RGB_CHANNELS {
                            rgba_into_rgb(img.data)
                        } else {
                            img.data
                        },
                        img.width,
                        img.height,
                        RGB_CHANNELS,
                    )
                        .unwrap(),
                    image::LoadResult::ImageF32(_) => {
                        panic!("Unknown image format: {}", path.display())
                    }
                    image::LoadResult::Error(e) => panic!("Failed to load image: {}", e),
                }
            })),
            coords,
            tolerance
        }
    }

    fn find_object(&self) -> PyResult<Option<[u16; 2]>> {
        let coords = self.matches_at_coords()?;
        if coords.is_some() {
            Ok(coords)
        } else {
            let screenshot = screen::get()?;
            let image = &self.image;
            let tolerance = self.tolerance;

            let coords = pixen::find_best(&*screenshot, image, tolerance);
            if let Some(coords) = coords {
                add_coords(&self.name, coords)?;
            }
            Ok(coords)
        }
    }

    fn is_on_screen(&self) -> PyResult<bool> {
        if self.matches_at_coords()?.is_some() {
            Ok(true)
        } else {
            let screenshot = screen::get()?;
            let image = &self.image;
            let tolerance = self.tolerance;

            Ok(pixen::matches(&*screenshot, image, tolerance))
        }
    }

    fn find_nth(&self, n: usize) -> PyResult<Option<[u16; 2]>> {
        let screenshot = screen::get()?;
        let image = &self.image;
        let tolerance = self.tolerance;

        Ok(pixen::find_nth(&*screenshot, image, tolerance, n))
    }

    fn matches_at_coords(&self) -> PyResult<Option<[u16; 2]>> {
        let screenshot = screen::get()?;
        let image = &self.image;
        let coords = &self.coords;
        let tolerance = self.tolerance;

        Ok(coords.iter().copied().find(|&c| pixen::matches_at(&*screenshot, image, c, tolerance)))
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
                [
                    [100, 200],
                    [100, 200],
                    [100, 200],
                    [100, 200],
                    [100, 200]
                ],
                5
            ),
            "delta": (
                [
                    [0, 0],
                    [0, 0],
                    [1, 1],
                    [1, 1],
                    [2, 2]
                ],
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

        assert_eq!(coords, HashSet::from([[100, 200]]));
    }

    #[test]
    fn get_objects_without_data() {
        let obj = OBJECTS.get("delta").unwrap();
        assert_eq!(obj.coords, HashSet::from([[0, 0], [1, 1], [2, 2]]));
    }
}
