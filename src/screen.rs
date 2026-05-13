use std::sync::RwLock;
use pixen::Image;

use crate::adb;

pub(crate) const CHANNELS: usize = 3;

pub(super) static SCREENSHOT: RwLock<Option<Image>> =  RwLock::new(None);



pub(super) fn set() { 
    let mut guard = SCREENSHOT.write().unwrap();
    
    if guard.is_none() {
        let (w, h) = adb::dimensions();
        let rgba_bytes = adb::screencap();

        let mut rgb_bytes = Vec::with_capacity(rgba_bytes.len() / 4 * 3);
        for [r, g, b, _] in rgba_bytes.into_chunks::<4>().into_iter() {
            rgb_bytes.push(r);
            rgb_bytes.push(g);
            rgb_bytes.push(b);
        }

        let image = Image::new(
            rgb_bytes,
            w as usize,
            h as usize,
            CHANNELS
        );

        *guard = Some(image);
    }
}


pub(super) fn reset() {
    *SCREENSHOT.write().unwrap() = None;
}
