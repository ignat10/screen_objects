use pixen::ImageView;

#[inline]
pub(crate) fn rgb_to_view(rgb: &image::RgbImage) -> ImageView<'_> {
    ImageView {
        buffer: rgb.as_raw(),
        channels: 3,
        width: rgb.width() as usize,
        height: rgb.height() as usize,
    }
}
