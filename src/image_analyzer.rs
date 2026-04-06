use core::simd::*;
use core::simd::num::*;


use image;

use crate::Coords;


const CHUNK_SIZE: usize = 32;

const THRESHOLD: f32 = 2.0;



pub(super) fn images_match(image1: &image::GrayImage, image2: &image::GrayImage, tolerance: f32) -> bool {
    assert!(image1.dimensions() == image2.dimensions(),
        "Images must have the same dimensions for comparison"
    );

    let total_diff: u32 = image1.as_raw().iter()
        .zip(image2.as_raw().iter())
        .map(|(&a, &b)| (a as i16 - b as i16).abs() as u32)
        .sum();
    
    total_diff < ((image1.width() * image1.height() * u8::MAX as u32) as f32 * tolerance) as u32
}



pub(super) fn find_sample(
    screen: &image::GrayImage,
    sample: &image::GrayImage,
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
