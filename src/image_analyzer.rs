use crate::Coords;
use crate::screen::SCREEN;

use image::RgbImage;


const RGB_CHANNELS: usize = 3;


const TOLERANCE: f32 = 0.05;


pub(super) fn images_match(
    sample: &RgbImage,
    coords: Coords,
) -> bool {
    let guard = SCREEN.read().unwrap();
    let screen = guard.as_ref().unwrap();

    let screen_w = screen.width() as usize;
    let sample_w = sample.width() as usize;
    let sample_h = sample.height() as usize;

    let raw_screen = screen.as_raw();
    let raw_sample = sample.as_raw();
    
    let mut diff_sum: u32 = 0;
    for y in 0..sample_h {
        let screen_start = (y + coords.y as usize) * screen_w + coords.x as usize;
        let sample_start = y * sample_w;
        for x in 0..sample_w {
            for c in 0..RGB_CHANNELS {
                diff_sum += raw_screen[(screen_start + x) * RGB_CHANNELS + c]
                    .abs_diff(raw_sample[(sample_start + x) * RGB_CHANNELS + c]) as u32;
            }
        }
    };
    TOLERANCE > diff_sum as f32 / (sample.len() * u8::MAX as usize) as f32
}


pub(super) fn find_sample(
    sample: &RgbImage,
) -> Option<Coords> {
    let guard = SCREEN.read().unwrap();
    let screen = guard.as_ref().unwrap();

    let screen_w = screen.width() as usize;
    let sample_w = sample.width() as usize;

    let screen_h = screen.height() as usize;
    let sample_h = sample.height() as usize;

    let raw_screen = screen.as_raw();
    let raw_sample = sample.as_raw();

    let w_step = sample_w.isqrt(); // too big step
    let h_step = sample_h.isqrt();

    let mut min_diff = u32::MAX;
    let (mut best_x, mut best_y) = (0, 0);
    for y in 0..=screen_h - sample_h {
        for x in 0..=screen_w - sample_w {

            let mut diff_sum: u32 = 0;
            for sy in (0..sample_h).step_by(h_step) {
                let screen_start = (y + sy) * screen_w + x;
                let sample_start = sy * sample_w;
                for sx in (0..sample_w).step_by(w_step) {
                    for c in 0..RGB_CHANNELS {
                        diff_sum += raw_screen[(screen_start + sx) * RGB_CHANNELS + c]
                            .abs_diff(raw_sample[(sample_start + sx) * RGB_CHANNELS + c]) as u32;
                    }
                }
            }
            if diff_sum < min_diff {
                min_diff = diff_sum;
                (best_x, best_y) = (x, y);
            }
        }
    }
    let checked_bytes = (sample_w / w_step) * (sample_h / h_step) * RGB_CHANNELS;
    // println!("{}", min_diff as f32 / (checked_bytes * u8::MAX as usize) as f32);  // tolerance need
    
    return if TOLERANCE > min_diff as f32 / (checked_bytes * u8::MAX as usize) as f32 {
         Some(Coords {
            x: best_x as u16,
            y: best_y as u16
        })
    } else {
        None
    }
}
