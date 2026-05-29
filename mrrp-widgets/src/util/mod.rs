pub mod ring_buffer;
pub mod staging;

use egui::Color32;

pub fn color32_to_linrgba(color: Color32) -> [f32; 4] {
    egui::Rgba::from(color).to_array()
}

#[cfg(test)]
mod tests {
    use egui::Color32;

    use crate::util::color32_to_linrgba;

    #[test]
    fn color32_to_linrgba_does_alpha() {
        let c = color32_to_linrgba(Color32::TRANSPARENT);
        assert_eq!(c[3], 0.0);
    }
}
