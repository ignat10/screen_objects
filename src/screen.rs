use parking_lot::{Mutex, MutexGuard, MappedMutexGuard};

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


fn set(screen_guard: &mut MutexGuard<'static, Option<image::GrayImage>>) {
    let screen_bytes = adb::screencap();
    let (w, h) = adb::dimensions();

    let rgba_img = image::RgbaImage::from_raw(w, h, screen_bytes)
        .expect("Failed to create RGBA image from screencap data");

    let gray_img = image::DynamicImage::ImageRgba8(rgba_img).into_luma8();
    **screen_guard = Some(gray_img);
}
