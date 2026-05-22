#![feature(vec_into_chunks)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::{fs, io};
use std::path::PathBuf;
use std::sync::{OnceLock, LazyLock, RwLock};

use pyo3::prelude::*;
use walkdir::WalkDir;
use rayon::prelude::*;
use serde_json::from_reader;
use pixen::{Image, images_match, find_sample};
use png::{ColorType, Decoder, Encoder};

pub mod adb;
mod screen;
pub mod utils;

use crate::screen::CHANNELS;
use crate::utils::rgba_into_rgb;

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
    let dirs: Vec<PathBuf> = WalkDir::new(&samples_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.into_path())
        .filter(|dir| {
            dir.read_dir()
                .unwrap()
                .all(|entry| entry.unwrap().path().is_file())
        })
        .collect();

    DATA_PATH.set(samples_dir.parent().unwrap().to_path_buf().join("objects_data.json")).unwrap();
    let mut lock  = DATA.write().unwrap();

    let mut objects = HashMap::new();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_str().unwrap().to_string();
        if !lock.contains_key(&name) {
            lock.insert(name.clone(), Vec::new());
        }
        objects.insert(
            name,
            ScreenObject::new(dir)
        );
    }
    objects
}

#[pyclass]
struct ScreenObject {
    path: PathBuf,
    images: HashMap<OsString, OnceLock<Image>>
}


#[pymethods]
impl ScreenObject {
    fn tap(&mut self) -> bool {
        let coords: [u16; 2] = if let Some(data_coords) = self.coords() {
            data_coords
        } else if let Some(found_coords) = self.find_object() {
            found_coords
        } else {
            return false;
        };

        let sample = self.iter_images()
            .find_any(|_| true)
            .unwrap();
        let center: [u16; 2] = [
            coords[0] + sample.width() as u16 / 2,
            coords[1] + sample.height() as u16 / 2
        ];

        adb::tap(center);
        screen::reset();
        true
    }

    fn spam_tap(&mut self, n: u8, interval: f32) -> bool {
        for _ in 0..n {
            if !self.tap() {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_secs_f32(interval));
        }
        true
    }

    fn exists(&mut self) -> Option<bool> {
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();

        let coords = self.coords()?;

        Some(
            self.iter_images()
            .any(|img| images_match(&screenshot, &img, coords))
        )
    }

    fn add_sample(&mut self) {
        screen::set();

        let lock = screen::SCREENSHOT.read().unwrap();
        let screenshot = lock.as_ref().unwrap();

        let coords = self.coords()
            .expect("required coords to add a sample.");
        let sample = self.iter_images().find_any(|_| true)
            .expect("required at least 1 sample already in dir, to know size.");


        let x = coords[0] as usize;
        let y = coords[1] as usize;

        let screen_w = screenshot.width();

        let sample_w = sample.width();
        let h = sample.height();
        let c = sample.channels();

        let row_start = x * c;
        let row_end = row_start + sample.width() * c;

        let crop: Vec<u8> = screenshot.as_raw()
            .chunks_exact(screen_w * c)
            .skip(y)
            .take(h)
            .flat_map(|row| {
                row[row_start..row_end].to_vec()
            })
            .collect();

        let file = fs::File::create(&self.path.join("new_sample.png")).unwrap();
        let writer = io::BufWriter::new(file);

        let mut encoder = Encoder::new(writer, sample_w as u32, h as u32);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(crop.as_slice()).unwrap();
    }
}


impl ScreenObject {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            images: HashMap::new()
        }
    }
    fn init(&mut self) {
        for entry in fs::read_dir(&self.path).unwrap() {
            let entry = entry.unwrap();
            self.images.insert(entry.file_name(), OnceLock::new());
        }
    }

    pub(crate) fn name(&self) -> String {
        self.path.file_name().unwrap().to_str().unwrap().to_string()
    }

    pub(crate) fn coords(&self) -> Option<[u16; 2]> {
        let lock = DATA.read().unwrap();
        let coords_vec = lock.get(&self.name()).unwrap();

        if coords_vec.len() < 5 {
            return None;
        }

        for window in coords_vec.windows(2) {
            let [c1, c2] = window else { unreachable!() };
            if c1 != c2 {
                return None;
            }
        }
        coords_vec.first().cloned()
    }

    fn iter_images(&mut self) -> impl ParallelIterator<Item = &Image> {
        if self.images.is_empty() {
            self.init();
        }

        let path = self.path.clone();
        self.images.par_iter_mut().map(move |(key, cell)| {
            cell.get_or_init(|| {
                let file = fs::File::open(path.join(key)).unwrap();
                let reader = io::BufReader::new(file);

                let decoder = Decoder::new(reader);
                let mut reader = decoder.read_info().unwrap();

                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();

                buf.drain(info.buffer_size()..);

                let rgb = match info.color_type {
                    ColorType::Rgba => rgba_into_rgb(buf),
                    ColorType::Rgb => buf,
                    _ => panic!("unsupported color type")
                };

                Image::new(
                    rgb,
                    info.width,
                    info.height,
                    CHANNELS
                )
            })
        })
    }

    fn find_object(&mut self) -> Option<[u16; 2]> {
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();

        let coords = self
            .iter_images()
            .find_map_any(|img| {
                find_sample(screenshot, img)
            })?;

        DATA.write().unwrap().get_mut(&self.name()).unwrap().push(coords);
        let writer = io::BufWriter::new(fs::File::create(DATA_PATH.get().unwrap()).unwrap());
        serde_json::to_writer(writer, &*DATA.read().unwrap()).unwrap();

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
            let path = SAMPLES.path().join(obj);
            fs::create_dir(path).unwrap();
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
        let coords = obj.coords().unwrap();

        assert_eq!(coords, [100, 200]);
    }

    #[test]
    fn get_objects_without_data() {
        let obj = OBJECTS.get("delta").unwrap();
        assert!(obj.coords().is_none());
    }
}
