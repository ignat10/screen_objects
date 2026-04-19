use std::sync::{MappedMutexGuard, Mutex, MutexGuard};



use image;

use crate::adb;


static SCREEN: Mutex<Option<image::GrayImage>> =  Mutex::new(None);



pub(super) fn get() -> MappedMutexGuard<'static, image::GrayImage> {
    let mut screen_guard = SCREEN.lock();
    
    if screen_guard.as_ref().is_none() {
        set(&mut screen_guard);
    }
    
    MutexGuard::map(screen_guard, |opt| opt.as_mut().unwrap())
}


pub(super) fn get_crop(x: u32, y: u32, width: u32, height: u32) -> image::GrayImage {
    let screen_guard = get();
    
    image::imageops::crop_imm(&*screen_guard, x, y, width, height).to_image()
}


pub(super) fn reset() {
    *SCREEN.lock() = None;
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
