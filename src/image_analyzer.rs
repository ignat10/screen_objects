use crate::Coords;

use image::RgbImage;


const RGB_CHANNELS: usize = 3;


pub(super) fn images_match(
    screen: &RgbImage,
    sample: &RgbImage,
    coords: Coords,
    tolerance: f32,
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
    tolerance < total_diff as f32 / ((sample.len() * u8::MAX as usize) as f32)
}


pub(super) fn find_sample(
    screen: &RgbImage,
    sample: &RgbImage,
    tolerance: f32,
) -> Option<Coords> {
    let sx = screen.width() as usize;
    let sy = screen.height() as usize;

    let ix= sample.width() as usize;
    let iy = sample.height() as usize;

    let raw_screen = screen.as_raw();
    let raw_sample = sample.as_raw();

    for y in 0..(sy - iy) {
        for x in 0..(sx - ix) {
            let mut diff_sum: u32 = 0;
            let mut counter: usize = 0;
            for row in 0..iy {
                let start_screen = (y + row) * sx + x;
                let start_sample = row * ix;
                for (chunk_a, chunk_b) in
                    raw_sample[start_sample..start_sample + ix].chunks_exact(CHUNK_SIZE)
                    .zip(raw_screen[start_screen..start_screen + ix].chunks_exact(CHUNK_SIZE))
                {
                    let a = u8x32::from_slice(chunk_a);
                    let b = u8x32::from_slice(chunk_b);

                    diff_sum += (a.cast::<i16>() - b.cast::<i16>()).abs().reduce_sum() as u32;
                    counter += CHUNK_SIZE;
                }

                let result = (diff_sum as f32 / (counter * u8::MAX as usize) as f32).sqrt();

                if result <= tolerance {
                                    dbg!(result);
                    return Some(Coords {
                        x: x as u16,
                        y: y as u16
                    });
                }


                if result > tolerance * THRESHOLD {
                    break;
                }
                println!("second loop");
            }
        }
    }
    None
}
