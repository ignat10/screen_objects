use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread::sleep;
use std::time::Duration;

use pyo3::prelude::*;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json;

mod adb;
mod screen;
mod image_analyzer;



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
}


static SAMPLES: OnceLock<PathBuf> = OnceLock::new();

fn samples() -> &'static PathBuf {
    SAMPLES.get().unwrap()
}


#[pyfunction]
fn get_objects(samples_dir: PathBuf, objects: PathBuf, ip: String) -> HashMap<String, ScreenObject> {
    SAMPLES.set(samples_dir).unwrap();

    adb::device_config(ip);

    serde_json::from_reader(
        fs::File::open(objects)
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
    fn tap(&self, delay: Option<f32>, steps: Option<u16>, repeat: Option<u8>) {
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
        screen::set();
    
        let coords = if let Some(steps) = steps {
            let delta = self.delta.as_ref().unwrap();
            self.coords.unwrap().with_delta(delta, steps)
        } else {
            self.coords.unwrap()
        };

        self.iter_images()
            .any(|img| {
                image_analyzer::images_match(img, coords)
            })
    }

    fn tap_if_found(&mut self) -> PyResult<bool> {
        screen::set();

        let coords = self.iter_images()
            .find_map_any(|sample_image|
            image_analyzer::find_sample(sample_image)
        );
    
        if let Some(coords) = coords {
            let size = self.iter_images()
                .find_any(|_| true)
                .unwrap()
                .dimensions();
            let center = Coords {
                x: coords.x + size.0 as u16 / 2,
                y: coords.y + size.1 as u16 / 2
            };
            Python::attach(|py| py.check_signals())?;

            adb::tap(center);
            screen::reset();
            return Ok(true);
        } else {
            return Ok(false);
        }
    }

    fn find_object(&mut self) -> Option<(u16, u16)> {
        screen::set();

        let coords = self
            .iter_images()
            .find_map_any(|img| image_analyzer::find_sample(img))?;
        
        Some((coords.x, coords.y))
    }

    fn add_sample(&mut self) {
        screen::set();

        let lock = screen::SCREEN.read().unwrap();
        let scr = lock.as_ref().unwrap();

        let coords: Coords = self.coords
            .expect("required coords to add a sample.");
        let size = self.iter_images().find_any(|_| true)
            .expect("required at least 1 sample already in dir, to know size.").dimensions();
        let path = self.path.as_ref()
            .expect("required path to add a sample.");

        let crop = image::imageops::crop_imm(
            &*scr,
            coords.x as u32,
            coords.y as u32 ,
            size.0,
            size.1
        ).to_image();

        crop.save(samples().join(path).join("new_sample.png"))
            .expect("Failed to save sample");
    }
}


impl ScreenObject {
    fn init(&mut self) {
        let path = self.path.as_ref().unwrap();
        let samples_dir = samples().join(path);
        
        for entry in fs::read_dir(samples_dir).unwrap() {
            let entry = entry.unwrap();
            self._images.insert(entry.file_name(), None);
        }
    }

    fn iter_images(&mut self) -> impl ParallelIterator<Item = &image::RgbImage> {
        if self._images.is_empty() {
            self.init();
        }

        let path = samples().join(self.path.as_ref().unwrap());

        self._images.par_iter_mut().filter_map(move |(key, img)| {
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
