use std::sync::RwLock;
use pixen::Image;

use crate::adb;
use crate::utils::rgba_into_rgb;

pub(crate) const CHANNELS: u8 = 3;

pub(super) static SCREENSHOT: RwLock<Option<Image>> =  RwLock::new(None);



pub(super) fn set() { 
    let mut guard = SCREENSHOT.write().unwrap();
    
    if guard.is_none() {
        let (w, h) = adb::dimensions();
        let rgba_bytes = adb::screencap();

        let image = Image::new(
            rgba_into_rgb(rgba_bytes),
            w,
            h,
            CHANNELS
        );

        *guard = Some(image);
    }
}


pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}
