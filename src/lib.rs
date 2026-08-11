#![feature(iter_array_chunks)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};

use pixen::*;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::from_reader;
use walkdir::WalkDir;

mod adb;
pub mod utils;

use utils::*;

pub(crate) type Coords = Point;
const BASE_TOLERANCE: u8 = 5;
const MINUTE: f32 = 60.0;

#[pymodule]
mod screen_objects {
    use super::*;

    #[pymodule_export]
    use adb::Device;

    #[pymodule_export]
    use adb::DeviceObject;

    #[pymodule_export]
    use adb::get_devices;

    #[pymodule_export]
    use config_objects;

    #[pymodule_export]
    use config_regions;

    #[pymodule_export]
    use Direction;

    #[pymodule_export]
    use SwipeSpeed;
}

type ObjectData = HashMap<String, (Option<Point>, Option<String>, u8)>;
static OBJECTS_PATH: OnceLock<PathBuf> = OnceLock::new();
static OBJECTS_DATA: LazyLock<RwLock<ObjectData>> = LazyLock::new(|| {
    let path = OBJECTS_PATH
        .get()
        .expect("OBJECTS_PATH must be initialized");
    load_json(path)
});

type RegionData = HashMap<String, Region>;
static REGIONS_PATH: OnceLock<PathBuf> = OnceLock::new();
static REGIONS_DATA: LazyLock<RwLock<RegionData>> = LazyLock::new(|| {
    let path = REGIONS_PATH
        .get()
        .expect("REGIONS_PATH must be initialized");
    load_json(path)
});

fn load_json<T>(path: &PathBuf) -> T
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return T::default();
    }
    let file =
        fs::File::open(&path).expect(format!("Failed to open file {}", path.display()).as_str());
    let map = from_reader(file).unwrap_or_else(|_| {
        fs::remove_file(&path)
            .expect(format!("failed to delete data-file {}.", path.display()).as_str());
        T::default()
    });
    map
}

#[pyfunction]
fn config_regions(regions_dir: PathBuf) -> PyResult<()> {
    let parent = regions_dir.parent().unwrap();
    REGIONS_PATH
        .set(parent.join("regions.json"))
        .map_err(|_| PyRuntimeError::new_err("config_regions can only be called once"))?;

    let regions: HashMap<String, ScreenRegion> = sample_paths(&regions_dir)?
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_string();
            (name, ScreenRegion { path })
        })
        .collect();

    REGION_SAMPLES
        .set(regions)
        .map_err(|_| PyRuntimeError::new_err("config_regions can only be called once"))
}

static REGION_SAMPLES: OnceLock<HashMap<String, ScreenRegion>> = OnceLock::new();

pub(crate) fn screen_region(key: &str) -> PyResult<&ScreenRegion> {
    REGION_SAMPLES
        .get()
        .ok_or_else(|| PyException::new_err("call config_regions before calibrating regions"))?
        .get(key)
        .ok_or_else(|| PyValueError::new_err(format!("No region found with key: {key}")))
}

static SCREEN_OBJECTS: OnceLock<HashMap<String, ScreenObject>> = OnceLock::new();

pub(crate) fn screen_object(key: &str) -> PyResult<&ScreenObject> {
    SCREEN_OBJECTS
        .get()
        .ok_or_else(|| PyException::new_err("call config_objects before object actions"))?
        .get(key)
        .ok_or_else(|| PyValueError::new_err(format!("No object found with key: {key}")))
}

#[pyfunction]
#[pyo3(signature = (objects_dir, regions_dir = None))]
fn config_objects(objects_dir: PathBuf, regions_dir: Option<PathBuf>) -> PyResult<()> {
    let files: Vec<PathBuf> = sample_paths(&objects_dir)?.collect();
    let mut seen_files = HashSet::new();
    for file in files.iter().map(|p| p.file_stem().unwrap().to_owned()) {
        if !seen_files.insert(file.clone()) {
            return Err(PyValueError::new_err(format!(
                "duplicate object names: {}",
                file.display()
            )));
        }
    }

    let parent = objects_dir.parent().unwrap();
    OBJECTS_PATH
        .set(parent.join("objects.json"))
        .map_err(|_| PyValueError::new_err("config_objects can only be called once"))?;
    let mut objects_data = OBJECTS_DATA.write().unwrap();

    let regions_path = regions_dir.map(|dir| dir.parent().unwrap().join("regions.json"));
    let regions: Option<RegionData> = regions_path.map(|path| load_json(&path));

    let mut objects = HashMap::new();
    for file in files {
        let name = file.file_stem().unwrap().to_str().unwrap().to_string();

        if !objects_data.contains_key(&name) {
            objects_data.insert(name.clone(), (None, None, BASE_TOLERANCE));
        };
        let (coords, region_key, tolerance) = objects_data.get(&name).unwrap().clone();
        let region = match (&regions, region_key) {
            (Some(regs), Some(k)) => {
                if let Some(reg) = regs.get(&k).copied() {
                    Some(reg)
                } else {
                    return Err(PyValueError::new_err(format!(
                        "Region '{}' was not found. Available regions: {:?}",
                        k,
                        regs.keys().collect::<Vec<_>>()
                    )));
                }
            }
            (_, None) => None,
            (None, Some(k)) => {
                return Err(PyValueError::new_err(format!(
                    "Object references region '{k}', but no regions directory was provided",
                )));
            }
        };

        objects.insert(name, ScreenObject::new(file, coords, region, tolerance));
    }
    SCREEN_OBJECTS
        .set(objects)
        .map_err(|_| PyValueError::new_err("config_objects can only be called once"))
}

fn sample_paths(root: &PathBuf) -> PyResult<impl Iterator<Item = PathBuf>> {
    Ok(WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path()))
}

struct ScreenRegion {
    path: PathBuf,
}

impl ScreenRegion {
    fn calibrate(&self, screen: &Image) -> PyResult<()> {
        let image = load_image(&self.path)?;
        let name = self.path.file_stem().unwrap().to_str().unwrap().to_string();
        let region = get_region(screen, &image);

        REGIONS_DATA.write().unwrap().insert(name, region);
        save_regions()
    }
}

struct ScreenObject {
    path: PathBuf,
    image: OnceLock<PyResult<Image>>,
    coords: Option<Coords>,
    region: Option<Region>,
    tolerance: u8,
}

impl ScreenObject {
    fn calibrate(
        &self,
        screen: &Image,
        fixed: bool,
        region: Option<String>,
        n: Option<usize>,
    ) -> PyResult<()> {
        let image = self.image()?;
        let (tolerance, coords) = match (self.region, n) {
            (Some(region), Some(n)) => get_nth_tolerance_in_region(screen, image, region, n),
            (Some(region), None) => get_tolerance_in_region(screen, image, region),
            (None, Some(n)) => get_nth_tolerance(screen, image, n),
            (None, None) => get_tolerance(screen, image),
        };

        *OBJECTS_DATA.write().unwrap().get_mut(self.name()).unwrap() = (
            if fixed { Some(coords) } else { None },
            region,
            tolerance + BASE_TOLERANCE,
        );
        save_objects()
    }
}

#[pyclass(from_py_object)]
#[derive(Copy, Clone)]
enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn destination(&self, mut coords: Coords, distance: u16) -> Coords {
        match self {
            Direction::Left => coords[0] = coords[0].saturating_sub(distance),
            Direction::Right => coords[0] = coords[0].saturating_add(distance),
            Direction::Up => coords[1] = coords[1].saturating_sub(distance),
            Direction::Down => coords[1] = coords[1].saturating_add(distance),
        }
        coords
    }
}

#[pyclass(from_py_object)]
#[derive(Clone, Copy)]
enum SwipeSpeed {
    Slow,
    Normal,
    Fast,
    Turbo,
}

impl SwipeSpeed {
    const fn pixels_per_second(&self) -> f32 {
        match self {
            SwipeSpeed::Slow => 320.0,
            SwipeSpeed::Normal => 640.0,
            SwipeSpeed::Fast => 1280.0,
            SwipeSpeed::Turbo => 2560.0,
        }
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

    fn find_object(&self, screen: &Image) -> PyResult<Option<Coords>> {
        let image = self.image()?;
        let coords = if self.coords.is_some() {
            if self.matches_at_coords(screen)? {
                self.coords
            } else {
                None
            }
        } else {
            let image = image;
            let tolerance = self.tolerance;
            if let Some(region) = self.region {
                find_in_region(screen, image, region, tolerance)
            } else {
                find_best(screen, image, tolerance)
            }
        };
        Ok(coords.map(|coords| center_coords(coords, image)))
    }

    fn is_on_screen(&self, screen: &Image) -> PyResult<bool> {
        if self.coords.is_some() {
            Ok(self.matches_at_coords(screen)?)
        } else {
            let image = self.image()?;
            let tolerance = self.tolerance;

            Ok(if let Some(region) = self.region {
                matches_in_region(screen, image, region, tolerance)
            } else {
                matches(screen, image, tolerance)
            })
        }
    }

    fn count_on_screen(&self, screen: &Image) -> PyResult<usize> {
        let image = self.image()?;
        let tolerance = self.tolerance;
        Ok(if let Some(region) = self.region {
            count_in_region(screen, image, region, tolerance)
        } else {
            count(screen, image, tolerance)
        })
    }

    fn find_each(&self, screen: &Image) -> PyResult<Vec<Coords>> {
        let image = self.image()?;
        let tolerance = self.tolerance;
        Ok(if let Some(region) = self.region {
            find_all_in_region(screen, image, region, tolerance)
        } else {
            find_all(screen, image, tolerance)
        })
    }

    fn find_nth(&self, screen: &Image, n: usize) -> PyResult<Option<Coords>> {
        if self.coords.is_some() {
            Err(PyValueError::new_err(format!(
                "Tried to find nth {}, but it has fixed coords.",
                self.name()
            )))
        } else {
            let image = self.image()?;
            let tolerance = self.tolerance;

            Ok(if let Some(region) = self.region {
                find_nth_in_region(screen, image, region, tolerance, n)
            } else {
                find_nth(screen, image, tolerance, n)
            })
        }
    }

    fn matches_at_coords(&self, screen: &Image) -> PyResult<bool> {
        let coords = self.coords.ok_or(PyValueError::new_err(format!(
            "Not found coords for {}.",
            self.name()
        )))?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        Ok(matches_at(screen, image, coords, tolerance))
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
