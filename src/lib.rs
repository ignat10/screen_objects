#![feature(vec_into_chunks)]

use std::collections::HashMap;
use std::{fs, io};
use std::path::PathBuf;
use std::sync::{OnceLock, LazyLock, RwLock};

use pyo3::prelude::*;
use walkdir::WalkDir;
use serde_json::from_reader;
use stb_image::image;
use pixen::{Image, images_match, find_sample};

pub mod adb;
mod screen;
pub mod utils;

use screen::RGB_CHANNELS;
use utils::rgba_into_rgb;

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
    fn tap(&self) -> bool {
        if let Some(coords) = self.find_object() {
            let sample = &self.image;
            let center: [u16; 2] = [
                coords[0] + sample.width() as u16 / 2,
                coords[1] + sample.height() as u16 / 2
            ];

            adb::tap(center);
            screen::reset();
            true
        } else { false }
    }

    fn spam_tap(&mut self, n: u8, interval: f32) -> bool {
        if let Some(coords) = self.find_object() {
            let image = &self.image;
            let center: [u16; 2] = [
                coords[0] + image.width() as u16 / 2,
                coords[1] + image.height() as u16 / 2
            ];
            for _ in 0..n {
                adb::tap(center);
                std::thread::sleep(std::time::Duration::from_secs_f32(interval));
            }
            screen::reset();
            true
        } else { false }
    }

    fn exists(&mut self) -> bool {
        self.find_object().is_some()
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
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();
        let image = &self.image;

        let coords = self.coords
            .filter(|&c| images_match(screenshot, image, c))
            .or_else(|| find_sample(screenshot, &self.image))
            ?;

        DATA.write().unwrap().get_mut(&self.name).unwrap().push(coords);
        let writer = io::BufWriter::new(fs::File::create(DATA_PATH.get().unwrap()).unwrap());
        serde_json::to_writer_pretty(writer, &*DATA.read().unwrap()).unwrap();

        Some(coords)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::{ json, Value, to_string_pretty };
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
