use std::sync::{RwLock, RwLockReadGuard, MappedRwLockReadGuard};
use pixen::Image;

use crate::adb;
use crate::utils::rgba_into_rgb;

pub(crate) const RGB_CHANNELS: usize = 3;

static SCREENSHOT: RwLock<Option<Image>> =  RwLock::new(None);



pub(super) fn get() -> MappedRwLockReadGuard<'static, Image> {
    set();
    RwLockReadGuard::map(
        SCREENSHOT.read().unwrap(),
        |img| img.as_ref().unwrap()
    )
}


fn set() {
    let mut guard = SCREENSHOT.write().unwrap();
    
    if guard.is_none() {
        let [w, h] = *adb::DIMENTIONS;
        let rgba_bytes = adb::screencap();

        let image = Image::new(
            rgba_into_rgb(rgba_bytes),
            w as usize,
            h as usize,
            RGB_CHANNELS
        ).unwrap();

        *guard = Some(image);
    }
}


pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}
