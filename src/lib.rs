#![feature(vec_into_chunks)]
#![feature(mapped_lock_guards)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock, RwLock};

use pixen::Image;
use pyo3::prelude::*;
use serde_json::from_reader;
use stb_image::image;
use walkdir::WalkDir;

pub mod adb;
mod screen;
pub mod utils;

use screen::RGB_CHANNELS;
use utils::*;

#[pymodule]
mod screen_objects {
    use super::*;

    #[pymodule_export]
    use get_objects;

    #[pymodule_export]
    use ScreenObject;

    #[pyfunction]
    fn reset_screen() {
        screen::reset();
    }

    #[pyfunction]
    fn device_config(adb: PathBuf, ip: Option<String>) {
        adb::device_config(adb, ip);
    }
    
    #[pyfunction]
    fn back() {
        adb::back();
        screen::reset();
    }
}


static DATA_PATH: OnceLock<PathBuf> = OnceLock::new();
static DATA: LazyLock<RwLock<HashMap<String, Vec<[u16; 2]>>>> = LazyLock::new(|| {
    let path = DATA_PATH.get().expect("DATA_PATH must be initialized");

    if !path.exists() {
        fs::write(&path, "{}").expect("Failed to write empty file");
    }

    let file = fs::File::open(&path).expect("Failed to open file");

    let map: HashMap<String, Vec<[u16; 2]>> =
        from_reader(file).expect("Failed to parse JSON");

    RwLock::new(map)
});



#[pyfunction]
fn get_objects(samples_dir: PathBuf) -> HashMap<String, ScreenObject> {
    let files: Vec<PathBuf> = WalkDir::new(&samples_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect();

    DATA_PATH.set(samples_dir.parent().unwrap().to_path_buf().join("objects_data.json")).unwrap();
    let mut lock  = DATA.write().unwrap();

    let mut objects = HashMap::new();
    for file in files {
        let name = file.file_stem().and_then(|n| n.to_str()).unwrap().to_string();

        if !lock.contains_key(&name) {
            lock.insert(name.clone(), Vec::new());
        };
        let coords_vec = lock.get(&name).unwrap();
        let coords = if coords_vec.len() < 5 {
            None
        } else if coords_vec.windows(2).all(|w| w[0] == w[1]) {
            coords_vec.first().cloned()
        } else {
            None
        };

        if !lock.contains_key(&name) {
        }

        objects.insert(
            name,
            ScreenObject::new(
                file,
                coords
            )
        );
    }
    objects
}

#[pyclass]
struct ScreenObject {
    name: String,
    image: LazyLock<Image, Box<dyn FnOnce() -> Image + Send + Sync>>,
    coords: Option<[u16; 2]>
}


#[pymethods]
impl ScreenObject {
    fn exists(&self) -> bool {
        self.find_any().is_some()
    }

    fn tap(&self) -> bool {
        if let Some(coords) = self.find_object() {
            let center = center_coords(coords, &self.image);

            adb::tap(center);
            screen::reset();
            true
        } else {
            false
        }
    }

    fn tap_nth(&self, n: usize) -> bool {
        if let Some(coords) = self.find_nth(n) {
            let center = center_coords(coords, &self.image);
            adb::tap(center);
            screen::reset();
            true
        } else {
            false
        }
    }

    fn spam_tap(&self, n: u8, interval: f32) -> bool {
        if let Some(coords) = self.find_object() {
            let image = &self.image;
            let center = center_coords(coords, image);
            for _ in 0..n {
                adb::tap(center);
                std::thread::sleep(std::time::Duration::from_secs_f32(interval));
            }
            screen::reset();
            true
        } else {
            false
        }
    }
}


impl ScreenObject {
    fn new(path: PathBuf, coords: Option<[u16; 2]>) -> Self {
        Self {
            name: path.file_stem().and_then(|n| n.to_str()).unwrap().to_string(),
            coords,
            image: LazyLock::new(Box::new(move || {
                let img = image::load(&path);
                match img {
                    image::LoadResult::ImageU8(img) => {
                        Image::new(
                            if img.depth != RGB_CHANNELS {
                                rgba_into_rgb(img.data)
                            } else {
                                img.data
                            },
                            img.width,
                            img.height,
                            RGB_CHANNELS
                        ).unwrap()
                    },
                    image::LoadResult::ImageF32(_) => panic!("Unknown image format: {}", path.display()),
                    image::LoadResult::Error(e) => panic!("Failed to load image: {}", e),
                }
            }))
        }
    }

    fn find_object(&self) -> Option<[u16; 2]> {
        let screenshot= screen::get();

        let coords = self.coords
            .filter(|&c| pixen::images_match(&*screenshot, &self.image, c))
            .or_else(|| pixen::find_best(&*screenshot, &self.image))
            ?;

        add_coords(&self.name, coords);
        Some(coords)
    }

    fn find_any(&self) -> Option<[u16; 2]> {
        let screenshot = screen::get();

        let coords = self.coords
            .filter(|&c| pixen::images_match(&*screenshot, &self.image, c))
            .or_else(|| pixen::find_first(&*screenshot, &self.image))
            ?;

        add_coords(&self.name, coords);
        Some(coords)
    }

    fn find_nth(&self, n: usize) -> Option<[u16; 2]> {
        let screenshot = screen::get();

        let coords = self.coords
            .filter(|&c| pixen::images_match(&*screenshot, &self.image, c))
            .or_else(|| pixen::find_nth(&*screenshot, &self.image, n))
            ?;

        add_coords(&self.name, coords);
        Some(coords)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::{json, to_string_pretty, Value};
    use tempfile::{tempdir, TempDir};


    static SAMPLES: LazyLock<TempDir> = LazyLock::new(|| tempdir().unwrap());
    static DATA: LazyLock<Value> = LazyLock::new(|| {
        json!({
            "alpha": [
                [100, 200],
                [100, 200],
                [100, 200],
                [100, 200],
                [100, 200]
            ],
            "delta": [
                [0, 0],
                [0, 0],
                [1, 1],
                [1, 1],
                [2, 2]
            ]
        })
    });

    static OBJECTS: LazyLock<HashMap<String, ScreenObject>> = LazyLock::new(|| {
        for obj in DATA.as_object().unwrap().keys() {
            let path = SAMPLES.path().join(format!("{}.png", obj));
            fs::File::create(path).unwrap();
        }
        fs::write(
            SAMPLES.path().parent().unwrap().join("objects_data.json"),
            to_string_pretty(&*DATA).unwrap()
        ).unwrap();
        get_objects(SAMPLES.path().to_path_buf())
    });

    #[test]
    fn get_objects_with_data() {
        let obj = OBJECTS.get("alpha").unwrap();
        let coords = obj.coords.unwrap();

        assert_eq!(coords, [100, 200]);
    }

    #[test]
    fn get_objects_without_data() {
        let obj = OBJECTS.get("delta").unwrap();
        assert!(obj.coords.is_none());
    }
}
