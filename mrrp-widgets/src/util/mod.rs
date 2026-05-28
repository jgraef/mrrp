pub mod ring_buffer;
pub mod staging;

use egui::Color32;

pub fn color32_to_linrgba(color: Color32) -> [f32; 4] {
    egui::Rgba::from(color).to_array()
}
