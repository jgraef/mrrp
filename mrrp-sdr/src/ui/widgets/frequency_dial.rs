use std::hash::Hash;

use egui::{
    Align,
    Align2,
    Color32,
    CursorIcon,
    FontId,
    MouseWheelUnit,
    Sense,
    Stroke,
    Style,
    TextFormat,
    Vec2,
    text::{
        LayoutJob,
        LayoutSection,
        TextWrapping,
    },
};

#[derive(Debug)]
pub struct FrequencyDial<'a> {
    frequency: &'a mut i64,
    num_digits: usize,
    insignificant_digits: Option<usize>,
    style: Option<FrequencyDialStyle>,
    id: Option<egui::Id>,
    desired_size: Vec2,
}

impl<'a> FrequencyDial<'a> {
    pub fn new(frequency: &'a mut i64) -> Self {
        Self {
            frequency,
            num_digits: 12,
            insignificant_digits: None,
            style: None,
            id: None,
            desired_size: Vec2::new(0.0, 0.0),
        }
    }

    pub fn digits(mut self, digits: usize) -> Self {
        self.num_digits = digits;
        self
    }

    pub fn insignificant_digits(mut self, digits: usize) -> Self {
        self.insignificant_digits = Some(digits);
        self
    }

    pub fn style(mut self, style: FrequencyDialStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn id(mut self, id: impl Hash) -> Self {
        self.id = Some(egui::Id::new(id));
        self
    }

    pub fn desired_size(mut self, size: Vec2) -> Self {
        self.desired_size = size;
        self
    }

    pub fn desired_width(mut self, width: f32) -> Self {
        self.desired_size.x = width;
        self
    }

    pub fn desired_height(mut self, height: f32) -> Self {
        self.desired_size.y = height;
        self
    }
}

impl<'a> egui::Widget for FrequencyDial<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // only used for the debug_assert at the end
        let frequency_before = *self.frequency;

        let id = self.id.unwrap_or_else(|| ui.id().with("frequency_dial"));

        // data needed to track superficial widget state, such as if it's being edited
        let data_id = id.with("data");
        let mut data = ui.data(|data_storage| {
            data_storage
                .get_temp::<WidgetData>(data_id)
                .unwrap_or_default()
        });
        let mut data_changed = false;

        let style = self
            .style
            .unwrap_or_else(|| FrequencyDialStyle::from_egui(&ui.style()));

        // track if the frequency changed.
        // we could also just check at the end if it's unequal to before, but with this
        // explicit flag we can mark_changed in the response even if it technically
        // didn't change.
        let mut frequency_changed = false;

        // track if we consumed the scroll delta
        let mut consumed_scroll_delta = false;

        // handle inputs (mouse wheel, keys)
        let mut scroll_delta = 0;
        ui.input(|input_state| {
            for event in &input_state.raw.events {
                match event {
                    // determine if there is scroll wheel input
                    egui::Event::MouseWheel {
                        unit,
                        delta,
                        phase,
                        modifiers,
                    } => {
                        // check smooth_scroll_delta to know if scrolling has already been consumed
                        if input_state.smooth_scroll_delta.y != 0.0 {
                            tracing::debug!(?unit, ?delta, ?phase, ?modifiers, "mouse wheel");

                            // not sure if we need to ignore page scrolling
                            if *unit == MouseWheelUnit::Line || *unit == MouseWheelUnit::Point {
                                if delta.y > 0.0 {
                                    scroll_delta += 1;
                                }
                                else if delta.y < 0.0 {
                                    scroll_delta -= 1;
                                }
                            }
                        }
                    }
                    // check if enter/escape are pressed
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => {
                        match key {
                            egui::Key::Enter => {
                                if let Some(edit_state) = &data.edit_state {
                                    if modifiers.alt {
                                        *self.frequency = zero_remaining_digits(
                                            *self.frequency,
                                            edit_state.digit_index + 1,
                                        );
                                        frequency_changed = true;
                                    }

                                    data.edit_state = None;
                                    data_changed = true;
                                }
                            }
                            egui::Key::Escape => {
                                if let Some(edit_state) = &data.edit_state {
                                    *self.frequency = edit_state.reset_value;
                                    data.edit_state = None;
                                    data_changed = true;
                                    frequency_changed = true;
                                }
                            }
                            egui::Key::Backspace => {
                                if let Some(edit_state) = &mut data.edit_state {
                                    *self.frequency = replace_digit_from(
                                        *self.frequency,
                                        edit_state.digit_index,
                                        edit_state.reset_value,
                                    );

                                    edit_state.digit_index += 1;

                                    data_changed = true;
                                    frequency_changed = true;
                                }
                            }
                            egui::Key::ArrowLeft => {
                                if let Some(edit_state) = &mut data.edit_state {
                                    // todo: shift-left/right to jump to next 1000s position.
                                    if modifiers.ctrl {
                                        let num_digits = ilog10(*self.frequency).saturating_sub(1);
                                        edit_state.digit_index = num_digits;
                                    }
                                    else {
                                        if edit_state.digit_index < self.num_digits {
                                            edit_state.digit_index += 1;
                                        }
                                        else {
                                            // todo: increase the displayed
                                            // number of digits. this should
                                            // only be visible while the digit
                                            // is selected. so when rendering
                                            // the text we should just take the
                                            // max of self.num_digits and
                                            // edit_state.digit_index
                                        }
                                    }

                                    data_changed = true;
                                    frequency_changed = true;
                                }
                            }
                            egui::Key::ArrowRight => {
                                if let Some(edit_state) = &mut data.edit_state {
                                    if edit_state.digit_index > 0 {
                                        if modifiers.ctrl {
                                            edit_state.digit_index = 0;
                                        }
                                        else {
                                            edit_state.digit_index -= 1;
                                        }
                                    }

                                    data_changed = true;
                                    frequency_changed = true;
                                }
                            }
                            egui::Key::ArrowUp | egui::Key::Plus => {
                                if let Some(edit_state) = &mut data.edit_state {
                                    let step = ipow10(edit_state.digit_index);
                                    *self.frequency += step;
                                    frequency_changed = true;
                                }
                            }
                            egui::Key::ArrowDown | egui::Key::Minus => {
                                if let Some(edit_state) = &mut data.edit_state {
                                    let step = ipow10(edit_state.digit_index);
                                    if *self.frequency >= step {
                                        *self.frequency -= step;
                                        frequency_changed = true;
                                    }
                                }
                            }

                            _ => {}
                        }
                    }
                    // check if text was input
                    egui::Event::Text(text) => {
                        if let Some(edit_state) = &mut data.edit_state {
                            for ch in text.chars() {
                                if let Some(value) = ch.to_digit(10) {
                                    *self.frequency = replace_digit(
                                        *self.frequency,
                                        edit_state.digit_index,
                                        value.into(),
                                    );

                                    data_changed = true;
                                    frequency_changed = true;

                                    if edit_state.digit_index == 0 {
                                        // turns out this feels unnatural
                                        //data.edit_state = None;
                                        //break;
                                    }
                                    else {
                                        edit_state.digit_index -= 1
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
        });

        // layout
        let (layout_job, selections) = {
            let num_characters = self.num_digits + self.num_digits.div_ceil(3) + 1;

            let mut layout_job = LayoutJob {
                text: String::with_capacity(num_characters),
                sections: Vec::with_capacity(num_characters),
                wrap: TextWrapping::no_max_width(),
                first_row_min_height: 0.0,
                break_on_newline: false,
                halign: Align::Min,
                justify: false,
                round_output_to_gui: false,
            };

            let frequency = *self.frequency;
            let mut frequency_abs = frequency.abs();

            let mut digit_index = 0;

            // individual characters (digits, decimals, sign) with layout sections
            // we need to create this ahead of time to properly reverse it and convert it to
            // text.
            let mut text_chars = Vec::with_capacity(num_characters);

            // similar to text_chars, but only tracks what digit this character corresponds
            // to
            let mut selections = Vec::with_capacity(num_characters);

            let mut layout_section = |ch: char, highlight, weak, smol, select_info| {
                let font_id = if smol {
                    style.small_font_id.clone()
                }
                else {
                    style.font_id.clone()
                };

                let mut format = TextFormat {
                    font_id,
                    extra_letter_spacing: style.digit_spacing,
                    line_height: None,
                    color: style.text_color,
                    background: style.background_color,
                    expand_bg: style.digit_expand_background,
                    coords: Default::default(),
                    italics: false,
                    underline: Stroke::NONE,
                    strikethrough: Stroke::NONE,
                    valign: Align::BOTTOM,
                };

                if highlight {
                    format.color = style.edit_text_color;
                    format.background = style.edit_background_color;
                }
                else if weak {
                    format.color = style.leading_zeros_text_color;
                }

                let layout_section = LayoutSection {
                    leading_space: 0.0,
                    byte_range: 0..0, // placeholder, fixed later
                    format,
                };

                text_chars.push((ch, layout_section));
                selections.push(select_info);
            };

            while frequency_abs > 0 || digit_index < self.num_digits {
                let digit_value = frequency_abs % 10;

                // this digit is being edited
                let is_being_edited = data
                    .edit_state
                    .as_ref()
                    .is_some_and(|edit_state| edit_state.digit_index == digit_index);

                // the remaining digits are all 0 and will be displayed in a lighter
                // tone
                let remaining_is_zero = frequency_abs == 0;

                // these will be rendered with a smaller font size
                let is_insignificant_digit =
                    self.insignificant_digits.is_some_and(|n| digit_index < n);

                // decimal point
                if digit_index > 0 && digit_index % 3 == 0 {
                    let previous_is_insignificant_digit = self
                        .insignificant_digits
                        .is_some_and(|n| digit_index < n + 1);

                    layout_section(
                        style.decimal, // decimal point
                        false,
                        remaining_is_zero,
                        previous_is_insignificant_digit,
                        None,
                    );
                }

                // layout section
                layout_section(
                    char::from_digit(digit_value.try_into().unwrap(), 10).unwrap(),
                    is_being_edited,
                    remaining_is_zero,
                    is_insignificant_digit,
                    Some((digit_index, digit_value)),
                );

                frequency_abs /= 10;
                digit_index += 1;
            }

            // draw minus sign
            // todo: this needs to be editable too. but we could also clamp to positive
            if frequency < 0 {
                layout_section('-', false, false, false, None);
            }

            // we created sections from right-to-left. now we fix this
            for (ch, mut layout_section) in text_chars.into_iter().rev() {
                layout_section.byte_range.start = layout_job.text.len();
                layout_job.text.push(ch);
                layout_section.byte_range.end = layout_job.text.len();
                layout_job.sections.push(layout_section);
            }
            selections.reverse();

            (layout_job, selections)
        };

        // draw and react to response
        let mut response = {
            let galley = ui.painter().layout_job(layout_job);
            assert_eq!(galley.rows.len(), 1);

            let desired_size = self.desired_size.max(galley.size());

            let (mut response, painter) =
                ui.allocate_painter(desired_size, Sense::CLICK | Sense::HOVER);

            response = response.on_hover_cursor(CursorIcon::Text);

            let aligned = style
                .align
                .align_size_within_rect(galley.size(), response.rect);

            painter.galley(aligned.min, galley.clone(), Color32::WHITE);

            // Helper to map cursor position to (digit_index, digit_value, step/multiplier)
            let cursor_position_to_digit = |cursor_position| {
                let row = &galley.rows[0];
                let position: Vec2 = cursor_position - aligned.min - row.pos.to_vec2();

                row.glyphs
                    .iter()
                    .enumerate()
                    .find_map(|(character_index, glyph)| {
                        // you can also match via rect() or logical_rect(), but this way we only
                        // consider the x coordinate
                        if position.x >= glyph.pos.x
                            && position.x < glyph.pos.x + glyph.advance_width
                        {
                            selections[character_index]
                        }
                        else {
                            None
                        }
                    })
            };

            if let Some(cursor_position) = response.interact_pointer_pos()
                && response.clicked()
                && let Some((digit_index, _digit_value)) = cursor_position_to_digit(cursor_position)
            {
                if let Some(edit_state) = &mut data.edit_state {
                    edit_state.digit_index = digit_index;
                }
                else {
                    data.edit_state = Some(EditState {
                        digit_index,
                        reset_value: *self.frequency,
                    });
                }
                data_changed = true;
            }

            if let Some(cursor_position) = response.hover_pos()
                && let Some((digit_index, _digit_value)) = cursor_position_to_digit(cursor_position)
            {
                let step = ipow10(digit_index);

                if scroll_delta > 0 {
                    *self.frequency += step;
                    frequency_changed = true;
                    consumed_scroll_delta = true;
                }
                else if scroll_delta < 0 && *self.frequency >= step {
                    *self.frequency -= step;
                    frequency_changed = true;
                    consumed_scroll_delta = true;
                }
            }

            response
        };

        debug_assert!(
            *self.frequency == frequency_before || frequency_changed,
            "frequency_changed boolean not set, but frequency changed"
        );

        // first we auto-confirmed when the cursor left, but i think it's more intuitive
        // to do this when you click outside
        //
        // !response.contains_pointer()
        if response.clicked_elsewhere() && data.edit_state.is_some() {
            tracing::debug!("not hovered anymore. resetting edit state");
            data.edit_state = None;
            data_changed = true;
        }

        if frequency_changed {
            response.mark_changed();
        }

        if consumed_scroll_delta {
            ui.input_mut(|input_state| {
                input_state.smooth_scroll_delta.y = 0.0;
            });
        }

        if data_changed {
            tracing::debug!(?data, "data changed");
            ui.data_mut(|data_storage| data_storage.insert_temp(data_id, data));
        }

        response
    }
}

#[derive(Clone, Debug)]
pub struct FrequencyDialStyle {
    pub font_id: FontId,
    pub small_font_id: FontId,
    pub text_color: Color32,
    pub background_color: Color32,
    pub digit_expand_background: f32,
    pub digit_spacing: f32,
    pub decimal: char,
    pub edit_text_color: Color32,
    pub edit_background_color: Color32,
    pub leading_zeros_text_color: Color32,
    pub align: Align2,
}

impl FrequencyDialStyle {
    pub fn from_egui(style: &Style) -> Self {
        Self {
            font_id: FontId::monospace(22.0),
            small_font_id: FontId::monospace(16.0),
            text_color: Color32::WHITE,
            background_color: Color32::TRANSPARENT,
            digit_expand_background: 0.0,
            digit_spacing: 2.0,
            decimal: '.',
            edit_text_color: Color32::BLACK,
            edit_background_color: Color32::WHITE,
            leading_zeros_text_color: style.visuals.weak_text_color(),
            align: Align2::CENTER_CENTER,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct WidgetData {
    edit_state: Option<EditState>,
}

#[derive(Clone, Debug)]
struct EditState {
    digit_index: usize,
    reset_value: i64,
}

fn ipow10(e: usize) -> i64 {
    assert!(e < 19, "Exponent too large");

    let mut y = 1;
    for _ in 0..e {
        y *= 10;
    }
    y
}

fn ilog10(mut x: i64) -> usize {
    let mut i = 0;
    while x > 0 {
        x /= 10;
        i += 1;
    }
    i
}

fn replace_digit(value: i64, digit: usize, new_value: i64) -> i64 {
    assert!(new_value >= 0 && new_value <= 9);

    // m = 10^i
    let m = ipow10(digit);

    // digits right of swapped
    let right = value % m;

    // the swapped digit but multiplied for correct position
    let swapped = new_value * m;

    // digits left of swapped
    let m2 = m * 10;
    let left = (value / m2) * m2;

    left + swapped + right
}

fn replace_digit_from(value: i64, digit: usize, from: i64) -> i64 {
    // m = 10^i
    let m = ipow10(digit);

    // digits right of swapped
    let right = value % m;

    // the swapped digit but multiplied for correct position
    let swapped = (from / m) % 10 * m;

    // digits left of swapped
    let m2 = m * 10;
    let left = (value / m2) * m2;

    left + swapped + right
}

fn zero_remaining_digits(value: i64, digit: usize) -> i64 {
    let m = ipow10(digit);
    (value / m) * m
}

#[cfg(test)]
mod tests {
    use crate::ui::widgets::frequency_dial::{
        replace_digit,
        replace_digit_from,
        zero_remaining_digits,
    };

    #[test]
    fn test_replace_digit() {
        assert_eq!(replace_digit(12345, 2, 9), 12945);
        assert_eq!(replace_digit(12345, 0, 9), 12349);
        assert_eq!(replace_digit(12345, 4, 9), 92345);
        assert_eq!(replace_digit(12345, 5, 9), 912345);
    }

    #[test]
    fn test_replace_digit_from() {
        assert_eq!(replace_digit_from(12345, 2, 98763), 12745);
        assert_eq!(replace_digit_from(12345, 0, 98763), 12343);
        assert_eq!(replace_digit_from(12345, 4, 98763), 92345);
        assert_eq!(replace_digit_from(12345, 5, 398763), 312345);
    }

    #[test]
    fn test_zero_remaining_digits() {
        assert_eq!(zero_remaining_digits(12345, 0), 12345);
        assert_eq!(zero_remaining_digits(12345, 1), 12340);
        assert_eq!(zero_remaining_digits(12345, 2), 12300);
        assert_eq!(zero_remaining_digits(12345, 3), 12000);
        assert_eq!(zero_remaining_digits(12345, 4), 10000);
        assert_eq!(zero_remaining_digits(12345, 5), 0);
        assert_eq!(zero_remaining_digits(12345, 6), 0);
    }
}
