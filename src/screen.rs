use std::sync::{MappedMutexGuard, Mutex, MutexGuard};



use image::{RgbImage, RgbaImage, DynamicImage};

use crate::adb;


static SCREEN: Mutex<Option<RgbImage>> =  Mutex::new(None);



pub(super) fn get() -> MappedMutexGuard<'static, RgbImage> {
    let mut guard = SCREEN.lock().unwrap();
    
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
    
    MutexGuard::map(guard, |opt| opt.as_mut().unwrap())
}


pub(super) fn reset() {
    *SCREEN.lock().unwrap() = None;
}
