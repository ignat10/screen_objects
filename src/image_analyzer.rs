use crate::Coords;

use image::RgbImage;


const RGB_CHANNELS: usize = 3;


const TOLERANCE: f32 = 0.1;


pub(super) fn images_match(
    screen: &RgbImage,
    sample: &RgbImage,
    coords: Coords,
) -> bool {
    let (w, h) = sample.dimensions();

    let start_row = coords.x as usize * RGB_CHANNELS;
    let end_row = start_row + w as usize * RGB_CHANNELS;
    let crop = screen
        .chunks_exact(screen.width() as usize * RGB_CHANNELS)
        .skip(coords.y as usize)
        .take(h as usize)
        .flat_map(|row| {
            &row[start_row..end_row]
        });

    let total_diff: u32 = sample.iter()
        .zip(crop)
        .map(|(&a, &b)| a.abs_diff(b) as u32)
        .sum();
    TOLERANCE > total_diff as f32 / ((sample.len() * u8::MAX as usize) as f32)
}


pub(super) fn find_sample(
    screen: &RgbImage,
    sample: &RgbImage,
) -> Option<Coords> {
    let screen_w = screen.width() as usize;
    let sample_w = sample.width() as usize;

    let screen_h = screen.height() as usize;
    let sample_h = sample.height() as usize;

    let raw_screen = screen.as_raw();
    let raw_sample = sample.as_raw();

    let step = sample.len().isqrt().isqrt(); // too big step

    let mut min_diff = u32::MAX;
    let (mut best_x, mut best_y) = (0, 0);
    for y in 0..=screen_h - sample_h {
        for x in 0..=screen_w - sample_w {

            let mut diff_sum: u32 = 0;
            for sy in (0..sample_h).step_by(step) {
                let screen_start = (y + sy) * screen_w + x;
                let sample_start = sy * sample_w;
                for sx in (0..sample_w).step_by(step) {
                    for c in 0..3 {
                        diff_sum += raw_screen[(screen_start + sx) * RGB_CHANNELS + c]
                            .abs_diff(raw_sample[(sample_start + sx) * RGB_CHANNELS + c]) as u32;
                    }
                }
            }
            if diff_sum < min_diff {
                println!("new best point: ({} {})", x, y);
                min_diff = diff_sum;
                (best_x, best_y) = (x, y);
            }
        }
    }
    let checked_bytes = (sample_w / step) * (sample_h / step) * RGB_CHANNELS;
    dbg!(checked_bytes);
    dbg!(step);
    dbg!(min_diff as usize / checked_bytes);
    dbg!(min_diff as f32 / (checked_bytes * u8::MAX as usize) as f32);
    
    return if TOLERANCE > min_diff as f32 / (checked_bytes * u8::MAX as usize) as f32 {
         Some(Coords {
            x: best_x as u16,
            y: best_y as u16
        })
    } else {
        None
    }
}
