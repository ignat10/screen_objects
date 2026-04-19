use std::sync::{MappedMutexGuard, Mutex, MutexGuard};



use image;

use crate::adb;


static SCREEN: Mutex<Option<image::RgbImage>> =  Mutex::new(None);



pub(super) fn get() -> MappedMutexGuard<'static, image::RgbImage> {
    let guard = SCREEN.lock().unwrap();
    
    if guard.as_ref().is_none() {
        drop(guard);
        set();
    }

    let guard = SCREEN.lock().unwrap();
    
    MutexGuard::map(guard, |opt| opt.as_mut().unwrap())
}


pub(super) fn reset() {
    *SCREEN.lock().unwrap() = None;
}


fn set() {
    let mut guard = SCREEN.lock().unwrap();

    let (w, h) = adb::dimensions();
    let output_bytes = adb::screencap();

    let rgba_img = image::RgbaImage::from_raw(
        w,
        h,
        output_bytes,
    ).expect("Failed to create RGB array from screencap bytes");

    let rgb_img = image::DynamicImage::from(rgba_img).to_rgb8();

    *guard = Some(rgb_img);
}
