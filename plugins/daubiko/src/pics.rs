//! Embedded picture and the pre-rendered photographic textures (see tools/render_textures.py).
use nih_plug_egui::egui::{Color32, ColorImage};

pub const PICTURE: &[u8] = include_bytes!("../assets/pic1.jpg");

pub const FACEPLATE: &[u8] = include_bytes!("../assets/ui/faceplate.jpg");
pub const WOOD: &[u8] = include_bytes!("../assets/ui/wood.jpg");
pub const VU_FACE: &[u8] = include_bytes!("../assets/ui/vu_face.jpg");
pub const KNOB_BASE: &[u8] = include_bytes!("../assets/ui/knob_base.png");
pub const KNOB_ROT: &[u8] = include_bytes!("../assets/ui/knob_rot.png");
pub const BIGKNOB_BASE: &[u8] = include_bytes!("../assets/ui/bigknob_base.png");
pub const BIGKNOB_ROT: &[u8] = include_bytes!("../assets/ui/bigknob_rot.png");
pub const TOGGLE_UP: &[u8] = include_bytes!("../assets/ui/toggle_up.png");
pub const TOGGLE_DOWN: &[u8] = include_bytes!("../assets/ui/toggle_down.png");
pub const LAMP_RED_ON: &[u8] = include_bytes!("../assets/ui/lamp_red_on.png");
pub const LAMP_RED_OFF: &[u8] = include_bytes!("../assets/ui/lamp_red_off.png");
pub const LAMP_AMBER_ON: &[u8] = include_bytes!("../assets/ui/lamp_amber_on.png");
pub const LAMP_AMBER_OFF: &[u8] = include_bytes!("../assets/ui/lamp_amber_off.png");
pub const SCREW: &[u8] = include_bytes!("../assets/ui/screw.png");

fn placeholder() -> ColorImage {
    ColorImage::new([4, 4], Color32::from_gray(60))
}

/// Decode an embedded JPEG; a small grey placeholder if decoding fails.
pub fn jpeg(data: &[u8]) -> ColorImage {
    let mut dec = zune_jpeg::JpegDecoder::new(data);
    if let Ok(px) = dec.decode() {
        if let Some((w, h)) = dec.dimensions() {
            if px.len() == w * h * 3 {
                return ColorImage::from_rgb([w, h], &px);
            }
        }
    }
    placeholder()
}

/// Decode an embedded PNG (RGBA or RGB, 8 bit).
pub fn png(data: &[u8]) -> ColorImage {
    let mut dec = zune_png::PngDecoder::new(data);
    if let Ok(px) = dec.decode_raw() {
        if let Some((w, h)) = dec.get_dimensions() {
            if px.len() == w * h * 4 {
                return ColorImage::from_rgba_unmultiplied([w, h], &px);
            }
            if px.len() == w * h * 3 {
                return ColorImage::from_rgb([w, h], &px);
            }
        }
    }
    placeholder()
}
