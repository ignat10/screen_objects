#![feature(mapped_lock_guards)]
#![feature(iter_array_chunks)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};

use pixen::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::from_reader;
use walkdir::WalkDir;
use serde::de::DeserializeOwned;

pub mod adb;
pub mod screen;
pub mod utils;

use utils::*;

pub(crate) type Coords = Point;
const BASE_TOLERANCE: u8 = 5;

#[pymodule]
mod screen_objects {
    use super::*;

    #[pymodule_export]
    use get_objects;

    #[pymodule_export]
    use ScreenObject;

    #[pymodule_export]
    use get_regions;

    #[pymodule_export]
    use ScreenRegion;

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
    
    #[pyfunction]
    fn swipe(start: &ScreenObject, end: &ScreenObject, duration: u16) -> PyResult<bool> {
        let start_coords = start.find_object()?;
        let end_coords = end.find_object()?;
        
        if let Some(start_coords) = start_coords && let Some(end_coords) = end_coords {
            adb::swipe(start_coords, end_coords, duration)?;
            Ok(true)
        } else { 
            Ok(false)
        }
    }
}

static OBJECTS_PATH: OnceLock<PathBuf> = OnceLock::new();
static OBJECTS_DATA: LazyLock<RwLock<HashMap<String, (Option<Point>, Option<String>, u8)>>> = LazyLock::new(|| {
    let path = OBJECTS_PATH.get().expect("OBJECTS_PATH must be initialized");
    load_data(path)
});

static REGIONS_PATH: OnceLock<PathBuf> = OnceLock::new();
static REGIONS_DATA: LazyLock<RwLock<HashMap<String, Region>>> = LazyLock::new(|| {
    let path = REGIONS_PATH.get().expect("REGIONS_PATH must be initialized");
    load_data(path)
});

fn load_data<T>(path: &PathBuf) -> RwLock<T>
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return RwLock::new(T::default());
    }
    let file = fs::File::open(&path).expect(format!("Failed to open file {}", path.display()).as_str());
    let map = from_reader(file)
        .unwrap_or_else(|_| {
            fs::remove_file(&path).expect(format!("failed to delete data-file {}.", path.display()).as_str());
            T::default()
        });
    RwLock::new(map)
}

#[pyfunction]
fn get_regions(regions_dir: PathBuf) -> PyResult<HashMap<String, ScreenRegion>> {
    let parent = regions_dir.parent().unwrap();
    REGIONS_PATH.set(parent.join("regions.json"))
        .map_err(|_| PyRuntimeError::new_err("get_regions can be called only once!"))?;

    Ok(
        walk_dir(&regions_dir)?
            .into_iter()
            .map(|path| {
                let name = path.file_stem().unwrap().to_str().unwrap().to_string();
                (name, ScreenRegion { path })
            })
            .collect()
    )
}

#[pyfunction]
#[pyo3(signature = (objects_dir, regions_dir = None))]
fn get_objects(objects_dir: PathBuf, regions_dir: Option<PathBuf>) -> PyResult<HashMap<String, ScreenObject>> {
    let files: Vec<PathBuf> = walk_dir(&objects_dir)?.collect();
    let mut seen_files = HashSet::new();
    for file in &files {
        if !seen_files.insert(file) {
            return Err(PyValueError::new_err(format!("duplicate object path: {}", file.display())));
        }
    }

    let parent = objects_dir.parent().unwrap();
    OBJECTS_PATH.set(parent.join("objects.json"))
        .map_err(|_| PyValueError::new_err("get_objects can be called only once."))?;
    let mut objects_data = OBJECTS_DATA.write().unwrap();

    let regions = if let Some(dir) = regions_dir {
        let parent = dir.parent().unwrap();
        if REGIONS_PATH.get().is_none() {
            REGIONS_PATH.set(parent.join("regions.json")).unwrap();
        }
        Some(REGIONS_DATA.read().unwrap())
    } else {
        None
    };

    let mut objects = HashMap::new();
    for file in files {
        let name = file
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        if !objects_data.contains_key(&name) {
            objects_data.insert(name.clone(), (None, None, BASE_TOLERANCE));
        };
        let (coords, region_key, tolerance) = objects_data.get(&name).unwrap().clone();
        let region = match (&regions, region_key) {
            (Some(regs), Some(k)) =>  {
                if let Some(reg) = regs.get(&k).copied() {
                   Some(reg) 
                } else {
                    return Err(PyValueError::new_err(format!("There is not region called '{}'. existing regions: {:?}", k, regs.keys().collect::<Vec<_>>())))
                }
            },
            (_, None) => None,
            (None, Some(k)) => return Err(PyValueError::new_err(format!("called region {}, but there is no regions loaded", k))),
        };

        objects.insert(name, ScreenObject::new(file, coords, region, tolerance));
    }
    Ok(objects)
}

fn walk_dir(root: &PathBuf) -> PyResult<impl Iterator<Item = PathBuf>> {
    Ok(
        WalkDir::new(root)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .into_iter()
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
    )
}
#[pyclass]
struct ScreenRegion {
    path: PathBuf,
}

#[pymethods]
impl ScreenRegion {
    fn calibrate(&self) -> PyResult<()> {
        let screenshot = screen::get()?;
        let image = load_image(&self.path)?;
        let name = self.path.file_stem().unwrap().to_str().unwrap().to_string();
        let region = get_region(&screenshot, &image);

        REGIONS_DATA.write()
            .unwrap()
            .insert(name, region);
        save_regions()
    }
}

#[pyclass]
struct ScreenObject {
    path: PathBuf,
    image: OnceLock<PyResult<Image>>,
    coords: Option<[u16; 2]>,
    region: Option<Region>,
    tolerance: u8,
}

#[pymethods]
impl ScreenObject {
    #[pyo3(signature = (fixed = false, region = None, n = None))]
    fn calibrate(&self, fixed: bool, region: Option<String>, n: Option<usize>) -> PyResult<()> {
        let screenshot = screen::get()?;
        let image = self.image()?;

        let (tolerance, coords) = match (self.region, n) {
            (Some(region), Some(n)) => get_nth_tolerance_in_region(&screenshot, image, region, n),
            (Some(region), None) => get_tolerance_in_region(&screenshot, image, region),
            (None, Some(n)) => get_nth_tolerance(&screenshot, image, n),
            (None, None) => get_tolerance(&screenshot, image)
        };

        *OBJECTS_DATA.write().unwrap().get_mut(self.name()).unwrap() = (
            if fixed {
                Some(coords)
            } else {
                None
            },
            region,
            tolerance + 1
        );
        save_objects()
    }

    fn exists(&self) -> PyResult<bool> {
        self.is_on_screen()
    }

    fn tap(&self) -> PyResult<bool> {
        if let Some(coords) = self.find_object()? {
            let center = center_coords(coords, self.image()?);
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
            let center = center_coords(coords, self.image()?);
            Python::attach(|py| py.check_signals())?;
            adb::tap(center)?;
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn count(&self) -> PyResult<usize> {
        let screenshot = screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        Ok(
            if let Some(region) = self.region {
                count_in_region(&*screenshot, image, region, tolerance)
            } else {
                count(&*screenshot, image, tolerance)
            }
        )
    }

    fn spam_tap(&self, n: u8, interval: f32) -> PyResult<bool> {
        if let Some(coords) = self.find_object()? {
            let image = self.image()?;
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

    fn debug(&self, point: Point) -> PyResult<()> {
        let screenshot = screen::get()?;
        println!("{:?}", debug_match(&screenshot, self.image()?, point));
        Ok(())
    }
}

impl ScreenObject {
    fn new(path: PathBuf, coords: Option<Point>, region: Option<Region>, tolerance: u8) -> Self {
        Self {
            path,
            image: OnceLock::new(),
            coords,
            region,
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
            let image = self.image()?;
            let tolerance = self.tolerance;
            Ok(
                if let Some(region) = self.region {
                    find_in_region(&screenshot, image, region, tolerance)
                } else {
                    find_best(&screenshot, image, tolerance)
                }
            )
        }
    }

    fn is_on_screen(&self) -> PyResult<bool> {
        if self.coords.is_some() {
            Ok(self.matches_at_coords()?)
        } else {
            let screenshot = screen::get()?;
            let image = self.image()?;
            let tolerance = self.tolerance;

            Ok(
                if let Some(region) = self.region {
                    matches_in_region(&*screenshot, image, region, tolerance)
                } else {
                    matches(&screenshot, image, tolerance)
                }
            )
        }
    }

    fn find_nth(&self, n: usize) -> PyResult<Option<[u16; 2]>> {
        if self.coords.is_some() {
            Err(PyValueError::new_err(format!("Tried to find nth {}, but it has fixed coords.", self.name())))
        } else {
            let screenshot = screen::get()?;
            let image = self.image()?;
            let tolerance = self.tolerance;

            Ok(
                if let Some(region) = self.region {
                    find_nth_in_region(&screenshot, image, region, tolerance, n)
                } else {
                    find_nth(&screenshot, image, tolerance, n)
                }
            )
        }
    }

    fn matches_at_coords(&self) -> PyResult<bool> {
        let coords = self.coords.ok_or(PyValueError::new_err(format!("Not found coords for {}.", self.name())))?;
        let screenshot = screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        Ok(matches_at(&*screenshot, image, coords, tolerance))
    }

    fn image(&self) -> PyResult<&Image> {
        self.image
            .get_or_init(|| load_image(&self.path))
            .as_ref()
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn name(&self) -> &str {
        self.path.file_stem().unwrap().to_str().unwrap()
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
                Value::Null,
                5
            ),
            "delta": (
                [1, 1],
                Value::Null,
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
        get_objects(SAMPLES.path().to_path_buf(), None).unwrap()
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
