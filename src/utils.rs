use pixen::ImageView;

pub(crate) fn rgb_to_view<'a>(rgb: &'a image::RgbImage) -> ImageView<'a> {
    ImageView {
        buffer: rgb.as_raw(),
        channels: 3,
        width: rgb.width() as usize,
        height: rgb.height() as usize,
    }
}
