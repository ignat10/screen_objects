#![feature(mapped_lock_guards)]
#![feature(iter_array_chunks)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};
use std::time::{Duration, Instant};

use pixen::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::from_reader;
use walkdir::WalkDir;

pub mod adb;
pub mod screen;
pub mod utils;

use utils::*;

pub(crate) type Coords = Point;
const BASE_TOLERANCE: u8 = 5;
const MINUTE: f32 = 60.0;

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

    #[pyfunction]
    fn screenshot() -> PyResult<()> {
        screen::save()
    }

    #[pyfunction]
    fn reset_screen() {
        screen::reset();
    }

    #[pyfunction]
    fn tap_center() -> PyResult<()> {
        let [w, h] = adb::DIMENSIONS.get().copied().ok_or_else(|| {
            PyRuntimeError::new_err("device_config must be called before tap_center")
        })?;

        adb::tap([w / 2, h / 2])?;
        screen::reset();
        Ok(())
    }

    #[pyfunction]
    fn swipe_center(dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<()> {
        let [w, h] = adb::DIMENSIONS.get().copied().ok_or_else(|| {
            PyRuntimeError::new_err("device_config must be called before swipe_center")
        })?;

        let start = [w / 2, h / 2];
        let distance = (speed.pixels_per_second() * duration) as u16;
        let end = dir.destination(start, distance);
        let time = (duration * 1000.0) as u16;
        screen::reset();
        adb::swipe(start, end, time)
    }

    #[pymodule_export]
    use adb::device_config;

    #[pymodule_export]
    use adb::start_app;

    #[pymodule_export]
    use adb::close_app;

    #[pyfunction]
    fn back() -> PyResult<()> {
        adb::back()?;
        screen::reset();
        Ok(())
    }

    #[pyfunction]
    fn home() -> PyResult<()> {
        adb::home()?;
        screen::reset();
        Ok(())
    }

    #[pyfunction]
    fn write(text: &str) -> PyResult<()> {
        adb::write(text)
    }

    #[pymodule_export]
    use Direction;

    #[pymodule_export]
    use SwipeSpeed;
}

static OBJECTS_PATH: OnceLock<PathBuf> = OnceLock::new();
static OBJECT_FILES: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ObjectLocation {
    Point(Point),
    Region(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RawLocation {
    Point(Point),
    Region(Region),
    Anywhere,
}

type ObjectData = (Option<ObjectLocation>, u8);

static OBJECTS_DATA: LazyLock<RwLock<HashMap<String, ObjectData>>> = LazyLock::new(|| {
    let path = OBJECTS_PATH
        .get()
        .expect("OBJECTS_PATH must be initialized");
    load_data(path)
});

static REGIONS_PATH: OnceLock<PathBuf> = OnceLock::new();
static REGIONS_DATA: LazyLock<RwLock<HashMap<String, Region>>> = LazyLock::new(|| {
    let path = REGIONS_PATH
        .get()
        .expect("REGIONS_PATH must be initialized");
    load_data(path)
});

fn load_data<T>(path: &PathBuf) -> T
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return T::default();
    }
    let file = fs::File::open(path)
        .unwrap_or_else(|error| panic!("failed to open data file '{}': {error}", path.display()));
    from_reader(file).unwrap_or_else(|_| {
        fs::remove_file(path).unwrap_or_else(|error| {
            panic!("failed to delete data file '{}': {error}", path.display())
        });
        T::default()
    })
}

#[pyfunction]
fn get_regions(regions_dir: PathBuf) -> PyResult<HashMap<String, ScreenRegion>> {
    let parent = regions_dir.parent().unwrap();
    REGIONS_PATH
        .set(parent.join("regions.json"))
        .map_err(|_| PyRuntimeError::new_err("get_regions can be called after get_objects!"))?;

    Ok(walk_dir(&regions_dir)?
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_string();
            (name, ScreenRegion { path })
        })
        .collect())
}

#[pyfunction]
#[pyo3(signature = (objects_dir, regions_dir = None))]
fn get_objects(
    objects_dir: PathBuf,
    regions_dir: Option<PathBuf>,
) -> PyResult<HashMap<String, ScreenObject>> {
    let files: Vec<PathBuf> = walk_dir(&objects_dir)?.collect();
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
        .map_err(|_| PyValueError::new_err("get_objects can be called only once."))?;
    OBJECT_FILES
        .set(
            files
                .iter()
                .map(|path| {
                    (
                        path.file_stem().unwrap().to_str().unwrap().to_string(),
                        path.clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| PyValueError::new_err("get_objects can be called only once."))?;
    let mut objects_data = OBJECTS_DATA.write().unwrap();

    let regions = regions_dir.map(|dir| {
        if REGIONS_PATH.get().is_none() {
            let parent = dir.parent().unwrap();
            REGIONS_PATH.set(parent.join("regions.json")).unwrap();
        }
        REGIONS_DATA.read().unwrap()
    });

    let mut objects = HashMap::new();
    for file in files {
        let file_name = file.file_stem().unwrap().to_str().unwrap().to_string();

        if !objects_data.contains_key(&file_name) {
            objects_data.insert(file_name.clone(), (None, BASE_TOLERANCE));
        };
        let (location, tolerance) = objects_data.get(&file_name).unwrap().clone();
        let raw_location = match location {
            Some(ObjectLocation::Point(point)) => RawLocation::Point(point),
            Some(ObjectLocation::Region(region_name)) => {
                let regions = regions.as_ref().ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "regions_dir is required because object '{}' uses region '{}'",
                        file_name, region_name
                    ))
                })?;
                let region = regions.get(&region_name).ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "object '{}' uses unknown region '{}'; available regions: {:?}",
                        file_name,
                        region_name,
                        regions.keys().collect::<Vec<_>>()
                    ))
                })?;
                RawLocation::Region(*region)
            }
            None => RawLocation::Anywhere,
        };

        objects.insert(file_name, ScreenObject::new(file, raw_location, tolerance));
    }
    Ok(objects)
}

fn walk_dir(root: &PathBuf) -> PyResult<impl Iterator<Item = PathBuf>> {
    Ok(WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path()))
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

        REGIONS_DATA.write().unwrap().insert(name, region);
        save_regions()
    }
}

#[pyclass]
struct ScreenObject {
    path: PathBuf,
    image: OnceLock<PyResult<Image>>,
    location: RawLocation,
    tolerance: u8,
}

#[pymethods]
impl ScreenObject {
    #[pyo3(signature = (fixed = false, region = None, n = None))]
    fn calibrate(&self, fixed: bool, region: Option<String>, n: Option<usize>) -> PyResult<()> {
        let screenshot = screen::get()?;
        let image = self.image()?;
        let raw_region = region
            .as_ref()
            .map(|r| {
                REGIONS_DATA
                    .read()
                    .unwrap()
                    .get(r)
                    .ok_or_else(|| PyValueError::new_err(format!("region '{}' was not found", r)))
                    .cloned()
            })
            .transpose()?;

        let (tolerance, point) = match (raw_region, n) {
            (Some(region), Some(n)) => get_nth_tolerance_in_region(&screenshot, image, region, n),
            (Some(region), None) => get_tolerance_in_region(&screenshot, image, region),
            (None, Some(n)) => get_nth_tolerance(&screenshot, image, n),
            (None, None) => get_tolerance(&screenshot, image),
        };

        let location = match (fixed, region) {
            (true, _) => Some(ObjectLocation::Point(point)),
            (false, Some(region)) => Some(ObjectLocation::Region(region)),
            (false, _) => None,
        };

        *OBJECTS_DATA.write().unwrap().get_mut(self.name()).unwrap() =
            (location, tolerance + BASE_TOLERANCE);
        save_objects()
    }

    fn exists(&self) -> PyResult<bool> {
        self.is_on_screen()
    }

    fn tap(&self) -> PyResult<bool> {
        if let Some(coords) = self.find_on_screen()? {
            let center = center_coords(coords, self.image()?);
            check_python_signals()?;
            adb::tap(center)?;
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_tap(&self) -> PyResult<()> {
        if !self.tap()? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn waitap(&self, timeout: f32) -> PyResult<bool> {
        if self.wait(timeout)? {
            self.tap()
        } else {
            Ok(false)
        }
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn force_waitap(&self, timeout: f32) -> PyResult<()> {
        if !self.waitap(timeout)? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }

    fn swipe(&self, dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<bool> {
        if let Some(start) = self.find_on_screen()? {
            let distance = (speed.pixels_per_second() * duration) as u16;
            let end = dir.destination(start, distance);
            let time = (duration * 1000.0) as u16;
            screen::reset();
            adb::swipe(start, end, time).map(|()| true)
        } else {
            Ok(false)
        }
    }

    fn force_swipe(&self, dir: Direction, speed: SwipeSpeed, duration: f32) -> PyResult<()> {
        if !self.swipe(dir, speed, duration)? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }

    fn tap_nth(&self, n: usize) -> PyResult<bool> {
        if let Some(coords) = self.find_nth(n)? {
            let center = center_coords(coords, self.image()?);
            check_python_signals()?;
            adb::tap(center)?;
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_tap_nth(&self, n: usize) -> PyResult<()> {
        if !self.tap_nth(n)? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }

    fn count(&self) -> PyResult<usize> {
        let screenshot = &screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        match self.location {
            RawLocation::Point(_) => Err(self.coords_error()),
            RawLocation::Region(region) => {
                Ok(count_in_region(screenshot, image, region, tolerance))
            }
            RawLocation::Anywhere => Ok(count(screenshot, image, tolerance)),
        }
    }

    fn tap_each(&self) -> PyResult<()> {
        let image = self.image()?;
        for point in self.find_all()? {
            let center = center_coords(point, image);
            check_python_signals()?;
            adb::tap(center)?;
            screen::reset();
        }
        Ok(())
    }

    fn spam_tap(&self, n: u8, interval: f32) -> PyResult<bool> {
        if let Some(coords) = self.find_on_screen()? {
            let image = self.image()?;
            let center = center_coords(coords, image);
            for _ in 0..n {
                adb::tap(center)?;
                std::thread::sleep(Duration::from_secs_f32(interval));
                check_python_signals()?;
            }
            screen::reset();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn force_spam_tap(&self, n: u8, interval: f32) -> PyResult<()> {
        if !self.spam_tap(n, interval)? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn wait(&self, timeout: f32) -> PyResult<bool> {
        let timeout = Duration::from_secs_f32(timeout);
        let start = Instant::now();
        while !self.exists()? {
            if start.elapsed() > timeout {
                return Ok(false);
            }
            check_python_signals()?;
            screen::reset();
        }
        Ok(true)
    }

    #[pyo3(signature = (timeout = MINUTE))]
    fn force_wait(&self, timeout: f32) -> PyResult<()> {
        if !self.wait(timeout)? {
            return Err(force_error(self.name()));
        }
        Ok(())
    }
}

fn check_python_signals() -> PyResult<()> {
    Python::attach(|py| py.check_signals())
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
        let [w, h] = *adb::DIMENSIONS.get().unwrap();
        match self {
            Direction::Left => coords[0] = coords[0].saturating_sub(distance),
            Direction::Right => coords[0] = coords[0].saturating_add(distance).min(w),
            Direction::Up => coords[1] = coords[1].saturating_sub(distance),
            Direction::Down => coords[1] = coords[1].saturating_add(distance).min(h),
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
    fn new(path: PathBuf, location: RawLocation, tolerance: u8) -> Self {
        Self {
            path,
            image: OnceLock::new(),
            location,
            tolerance,
        }
    }

    fn is_on_screen(&self) -> PyResult<bool> {
        let screenshot = &screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        Ok(match self.location {
            RawLocation::Point(point) => matches_at(screenshot, image, point, tolerance),
            RawLocation::Region(region) => matches_in_region(screenshot, image, region, tolerance),
            RawLocation::Anywhere => matches(screenshot, image, tolerance),
        })
    }

    fn find_on_screen(&self) -> PyResult<Option<Point>> {
        let screenshot = screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        Ok(match self.location {
            RawLocation::Point(point) => {
                if matches_at(&screenshot, image, point, tolerance) {
                    Some(point)
                } else {
                    None
                }
            }
            RawLocation::Region(region) => find_in_region(&screenshot, image, region, tolerance),
            RawLocation::Anywhere => find_best(&screenshot, image, tolerance),
        })
    }

    fn find_nth(&self, n: usize) -> PyResult<Option<[u16; 2]>> {
        let screenshot = screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;
        match self.location {
            RawLocation::Point(_) => Err(self.coords_error()),
            RawLocation::Region(region) => {
                Ok(find_nth_in_region(&screenshot, image, region, tolerance, n))
            }
            RawLocation::Anywhere => Ok(find_nth(&screenshot, image, tolerance, n)),
        }
    }

    fn find_all(&self) -> PyResult<Vec<Point>> {
        let screenshot = screen::get()?;
        let image = self.image()?;
        let tolerance = self.tolerance;

        match self.location {
            RawLocation::Point(_) => Err(self.coords_error()),
            RawLocation::Region(region) => {
                Ok(find_all_in_region(&screenshot, image, region, tolerance))
            }
            RawLocation::Anywhere => Ok(find_all(&screenshot, image, tolerance)),
        }
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

    fn coords_error(&self) -> PyErr {
        PyValueError::new_err(format!(
            "cannot use a multiple-position method on '{}': it has fixed coordinates",
            self.name()
        ))
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
            "alpha": [[100, 200], 5],
            "delta": [[1, 1], 6]
        })
    });
    static OBJECTS: LazyLock<HashMap<String, ScreenObject>> = LazyLock::new(|| {
        for obj in DATA.as_object().unwrap().keys() {
            let path = SAMPLES.path().join(format!("{}.png", obj));
            fs::File::create(path).unwrap();
        }
        fs::write(
            SAMPLES.path().parent().unwrap().join("objects.json"),
            to_string_pretty(&*DATA).unwrap(),
        )
        .unwrap();
        get_objects(SAMPLES.path().to_path_buf(), None).unwrap()
    });

    #[test]
    fn loads_alpha_location() {
        let obj = OBJECTS.get("alpha").unwrap();
        let coords = obj.location;

        assert_eq!(coords, RawLocation::Point([100, 200]));
    }

    #[test]
    fn loads_delta_location() {
        let obj = OBJECTS.get("delta").unwrap();
        let coords = obj.location;

        assert_eq!(coords, RawLocation::Point([1, 1]));
    }
}
