#![feature(vec_into_chunks)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::{fs, io};
use std::path::PathBuf;
use std::sync::OnceLock;

use pyo3::prelude::*;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json;
use pixen::{Image, images_match, find_sample};
use png::{Decoder, Encoder};
use crate::screen::CHANNELS;

mod adb;
mod screen;


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
    images: HashMap<OsString, OnceLock<Image>>
}


#[pymethods]
impl ScreenObject {
    #[pyo3(signature = (offset_steps=None))]
    fn tap(&self, offset_steps: Option<u16>) {
        let coords = self.coords.unwrap();
        let coords = if let Some(steps) = offset_steps {
            let delta = self.delta.as_ref().unwrap();
            coords.with_delta(delta, steps)
        } else {
            coords
        };

        adb::tap(coords);
        screen::reset();
    }

    fn spam_tap(&self, n: u8, interval: f32) {
        for _ in 0..n {
            self.tap(None);
            std::thread::sleep(std::time::Duration::from_secs_f32(interval));
        }
    }

    #[pyo3(signature = (offset_steps=None))]
    fn compare(&mut self, offset_steps: Option<u16>) -> bool {
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();

        let coords = self.coords.as_ref().unwrap();
        let coords = if let Some(steps) = offset_steps {
            let delta = self.delta.as_ref().unwrap();
            coords.with_delta(delta, steps)
        } else {
            coords.clone()
        };

        self.iter_images()
            .any(|img| {
                images_match(&screenshot, &img, coords.x as usize, coords.y as usize)
            })
    }

    fn tap_if_found(&mut self) -> PyResult<bool> {
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();


        let coords = self.iter_images()
            .find_map_any(|sample_image| {
                find_sample(screenshot, &sample_image)
            }
        );
    
        Ok(if let Some(coords) = coords {
            let sample = self.iter_images()
                .find_any(|_| true)
                .unwrap();
            let center = Coords {
                x: (coords.0 + sample.width() / 2) as u16,
                y: (coords.1 + sample.height() / 2) as u16
            };
            Python::attach(|py| py.check_signals())?;

            adb::tap(center);
            screen::reset();
            true
        } else {
            false
        })
    }

    fn find_object(&mut self) -> Option<(usize, usize)> {
        screen::set();
        let guard = screen::SCREENSHOT.read().unwrap();
        let screenshot = guard.as_ref().unwrap();

        let coords = self
            .iter_images()
            .find_map_any(|img| {
                find_sample(screenshot, img)
            })?;
        
        Some(coords)
    }

    fn add_sample(&mut self) {
        screen::set();

        let lock = screen::SCREENSHOT.read().unwrap();
        let screenshot = lock.as_ref().unwrap();

        let coords: Coords = self.coords
            .expect("required coords to add a sample.");
        let path = self.path.as_ref()
            .expect("required path to add a sample.")
            .clone();
        let sample = self.iter_images().find_any(|_| true)
            .expect("required at least 1 sample already in dir, to know size.");


        let x = coords.x as usize;
        let y = coords.y as usize;

        let w = screenshot.width();
        let h = sample.height();
        let c = sample.channels();

        let row_start = x * c;
        let row_end = row_start + sample.width() * c;

        let crop: Vec<u8> = screenshot.as_raw()
            .chunks_exact(w * c)
            .skip(y)
            .take(h)
            .flat_map(|row| {
                row[row_start..row_end].iter().copied()
            })
            .collect();

        let file = fs::File::create(path).unwrap();
        let writer = io::BufWriter::new(file);

        let mut encoder = Encoder::new(writer, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(crop.as_slice()).unwrap();
    }
}


impl ScreenObject {
    fn init(&mut self) {
        let path = self.path.as_ref().unwrap();
        let samples_dir = samples().join(path);
        
        for entry in fs::read_dir(samples_dir).unwrap() {
            let entry = entry.unwrap();
            self.images.insert(entry.file_name(), OnceLock::new());
        }
    }

    fn iter_images(&mut self) -> impl ParallelIterator<Item = &Image> {
        if self.images.is_empty() {
            self.init();
        }

        let path = samples().join(self.path.as_ref().unwrap());

        self.images.par_iter_mut().filter_map(move |(key, cell)| {
            cell.get_or_init(|| {
                let file = fs::File::open(path.join(key)).unwrap();
                let reader = io::BufReader::new(file);

                let decoder = Decoder::new(reader);
                let mut reader = decoder.read_info().unwrap();

                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();

                buf.drain(info.buffer_size()..);
                dbg!(buf.len(), info.buffer_size(), info.width, info.height, info.bit_depth, info.color_type, info.line_size);

                Image::new(
                    buf,
                    info.width as usize,
                    info.height as usize,
                    CHANNELS
                )
            });
            cell.get()
        })
    }
}


#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Coords {
    x: u16,
    y: u16,
}


impl Coords {
    fn with_delta(&self, delta: &Delta, steps: u16) -> Coords {
        let x = self.x;
        let y = self.y;

        let dir = &delta.dir;
        let offset = delta.gap * steps;
        Coords {
            x: match dir {
                Dir::Right => x + offset,
                Dir::Left => x - offset,
                _ => x
            },
            y: match dir {
                Dir::Up => y + offset,
                Dir::Down => y - offset,
                _ => y
            }
        }
    }
}


#[derive(Deserialize)]
struct Delta {
    dir: Dir,
    gap: u16
}


#[derive(Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}









#[cfg(test)]
mod tests {
    use super::*;


    const DATA: &str = r#"
        {
            "coords": {
                "x": 100,
                "y": 200
            },
            "delta": {
                "dir": "Right",
                "gap": 10
            },
            "path": "sample_path"
        }
    "#;
    #[test]
    fn screen_object_deserialization() {
        let result = serde_json::from_str::<ScreenObject>(DATA);

        assert!(result.is_ok(), "{}", result.err().unwrap());

        let screen_object = result.unwrap();

        let coords = screen_object.coords.unwrap();
        let delta = screen_object.delta.unwrap();
        let path = screen_object.path.unwrap();

        assert_eq!(coords.x, 100);
        assert_eq!(coords.y, 200);
        assert_eq!(delta.dir, Dir::Right);
        assert_eq!(delta.gap, 10);
        assert_eq!(path, PathBuf::from("sample_path"));
    }

    #[test]
    fn delta_test() {
        let obj: ScreenObject = serde_json::from_str(DATA).unwrap();

        let coords = obj.coords.unwrap();
        let delta = obj.delta.as_ref().unwrap();
        assert_eq!(coords.with_delta(delta, 0), coords);
        assert_eq!(coords.with_delta(delta, 1), Coords { x: coords.x + delta.gap, y: coords.y });
        assert_eq!(coords.with_delta(delta, 3), Coords { x: coords.x + delta.gap * 3, y: coords.y });
    }
}
