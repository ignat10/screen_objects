use crate::{DATA, DATA_PATH};
use pixen::Image;
use std::{fs, io};

pub(crate) fn rgba_into_rgb(rgba: Vec<u8>) -> Vec<u8> {
    assert_eq!(rgba.len() % 4, 0);
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);

    for [r, g, b, _] in rgba.into_chunks::<4>() {
        rgb.push(r);
        rgb.push(g);
        rgb.push(b);
    }
    rgb
}

pub(crate) fn add_coords(key: &str, val: [u16; 2]) {
    DATA.write().unwrap().get_mut(key).unwrap().push(val);
    let writer = io::BufWriter::new(fs::File::create(DATA_PATH.get().unwrap()).unwrap());
    serde_json::to_writer_pretty(writer, &*DATA.read().unwrap()).unwrap();
}

pub(super) fn center_coords(corner: [u16; 2], img: &Image) -> [u16; 2] {
    [
        corner[0] + img.width() as u16 / 2,
        corner[1] + img.height() as u16 / 2,
    ]
}
