#![feature(mapped_lock_guards)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use pyo3::prelude::*;
use serde::Deserialize;
use serde_json;

pub mod paths;
pub mod adb;
mod screen;
mod image_analyzer;



#[pymodule]
mod screen_objects {
    #[pymodule_export]
    use super::get_objects;

    #[pymodule_export]
    use super::ScreenObject;
}


#[pyfunction]
fn get_objects(data_path: PathBuf) -> HashMap<String, ScreenObject> {
    paths::init(PathBuf::from(data_path));

    adb::device_config();

    serde_json::from_reader(
        fs::File::open(paths::game_objects())
        .expect("Failed open file")
    )
    .expect("Failed to parse JSON data")
}


#[pyclass]
#[derive(Deserialize)]
struct ScreenObject {
    coords: Option<Coords>,
    delta: Option<Delta>,
    path: Option<PathBuf>,
    #[serde(skip)]
    _images: HashMap<OsString, Option<image::RgbImage>>
}


#[pymethods]
impl ScreenObject {
    #[pyo3(signature = (delay=None, steps=None, repeat=None))]
    fn tap(&mut self, delay: Option<f32>, steps: Option<u16>, repeat: Option<u8>) {
        let coords = if let Some(steps) = steps {
            let delta = self.delta.as_ref().unwrap();
            self.coords.unwrap().with_delta(delta, steps)
        } else {
            self.coords.unwrap()
        };

        if let Some(secs) = delay {
            sleep(Duration::from_secs_f32(secs))
        };

        for _ in 0..repeat.unwrap_or(1) {
            adb::tap(coords);
        }
        screen::reset();
    }


    #[pyo3(signature = (steps=None))]
    fn compare(&mut self, steps: Option<u16>) -> bool {
    
        let screen_img = screen::get();
        let coords = if let Some(steps) = steps {
            let delta = self.delta.as_ref().unwrap();
            self.coords.unwrap().with_delta(delta, steps)
        } else {
            self.coords.unwrap()
        };

        self.iter_images()
            .any(|img| {
                image_analyzer::images_match(&*screen_img, img, coords)
            })
    }

    fn tap_if_found(&mut self) -> bool {
        let screen = &*screen::get();

        let coords = self.iter_images()
            .find_map(|sample_image|
            image_analyzer::find_sample(screen, sample_image)
        );

        if let Some(coords) = coords {
            let size = self.iter_images()
                .next()
                .unwrap()
                .dimensions();
            let center = Coords {
                x: coords.x + size.0 as u16 / 2,
                y: coords.y + size.1 as u16 / 2
            };

            adb::tap(center);
            screen::reset();
            return true;
        } else {
            return false;
        }
    }

    fn find_object(&mut self) -> Option<(u16, u16)> {
        let screen = &*screen::get();
        let coords = self
            .iter_images()
            .find_map(|img| image_analyzer::find_sample(screen, img))?;
        
        Some((coords.x, coords.y))
    }
}


impl ScreenObject {
    fn init(&mut self) {
        let path = self.path.as_ref().unwrap();
        let samples_dir = paths::samples().join(path);
        
        for entry in fs::read_dir(samples_dir).unwrap() {
            let entry = entry.unwrap();
            self._images.insert(entry.file_name(), None);
        }
    }

    fn iter_images(&mut self) -> impl Iterator<Item = &image::RgbImage> {
        if self._images.is_empty() {
            self.init();
        }

        let path = paths::samples().join(self.path.as_ref().unwrap());

        self._images.iter_mut().filter_map(move |(key, img)| {
            if img.is_none() {
                *img = Some(
                    image::open(path.join(key))
                    .expect("Failed to open sample image")
                    .to_rgb8()
                );
            }

            img.as_ref()
        })
    }
}


#[derive(Deserialize, Copy, Clone)]
pub(crate) struct Coords {
    x: u16,
    y: u16,
}


impl Coords {
    fn with_delta(&self, delta: &Delta, steps: u16) -> Coords {
        let x = self.x;
        let y = self.y;

        match delta {
            Delta::PosX(interval) => Coords {
                x: x + interval * steps,
                y: y,
            },
            Delta::NegX(interval) => Coords {
                x: x - interval * steps,
                y: y,
            },
            Delta::PosY(interval) => Coords {
                x: x,
                y: y + interval * steps,
            },
            Delta::NegY(interval) => Coords {
                x: x,
                y: y - interval * steps
            }
        }
    }
}


#[derive(Deserialize, PartialEq, Debug)]
enum Delta {
    PosX(u16),
    NegX(u16),
    PosY(u16),
    NegY(u16),
}









#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn screen_object_deserialization() {
        let data = r#"
            {
                "coords": {
                    "x": 100,
                    "y": 200
                },
                "delta": {
                    "PosX": 10
                },
                "path": "sample_path"
            }
        "#;

        let result = serde_json::from_str::<ScreenObject>(&data);

        assert!(result.is_ok(), "{}", result.err().unwrap());

        let screen_object = result.unwrap();

        let coords = screen_object.coords.unwrap();
        let delta = screen_object.delta.unwrap();
        let path = screen_object.path.unwrap();

        assert_eq!(coords.x, 100);
        assert_eq!(coords.y, 200);
        assert_eq!(delta, Delta::PosX(10));
        assert_eq!(path, PathBuf::from("sample_path"));
    }
}
