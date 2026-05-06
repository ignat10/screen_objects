use std::sync::RwLock;
use image::{RgbImage, RgbaImage, DynamicImage};

use crate::adb;

pub(crate) const CHANNELS: usize = 3;

pub(super) static SCREENSHOT: RwLock<Option<RgbImage>> =  RwLock::new(None);



pub(super) fn set() { 
    let mut guard = SCREENSHOT.write().unwrap();
    
    if guard.is_none() {
        let (w, h) = adb::dimensions();
        let output_bytes = adb::screencap();

        let rgba_img = RgbaImage::from_raw(
            w,
            h,
            output_bytes,
        ).expect("Failed to create RGB array from screencap bytes");

        let rgb_img = DynamicImage::from(rgba_img).to_rgb8();

        *guard = Some(rgb_img);
    }
}


pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}
