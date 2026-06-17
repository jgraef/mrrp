use std::{
    borrow::Cow,
    convert::identity,
    fs::File,
    io::{
        BufReader,
        Read,
    },
    ops::{
        Bound,
        RangeBounds,
    },
    path::Path,
    sync::OnceLock,
};

use color_eyre::eyre::Error;
use palette::{
    Srgba,
    color_difference::Wcag21RelativeContrast,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::Widget,
};
use serde::{
    Deserialize,
    Deserializer,
};

use crate::util::FrequencyBand;

pub(crate) const BANDPLAN_INTERNATIONAL_BYTES: &'static [u8] = include_bytes!("bandplan.csv");

#[derive(Clone, Debug)]
pub struct Bandplan {
    bands: Vec<Band>,
    by_end: Vec<(u32, usize)>,
}

impl Bandplan {
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, Error> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .comment(Some(b'#'))
            .trim(csv::Trim::All)
            .flexible(true)
            .from_reader(reader);

        Ok(reader
            .deserialize::<Band>()
            .filter_map(Result::ok)
            .filter(|band| !band.name.is_empty())
            .collect())
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        tracing::debug!(path = %path.as_ref().display(), "Loading bandplan from file");
        Ok(Bandplan::from_reader(BufReader::new(File::open(path)?))?)
    }

    #[inline]
    pub fn get(&self, frequency: u32) -> Option<&Band> {
        let index = self.start_index(frequency);
        self.bands.get(index)
    }

    #[inline]
    pub fn get_many(&self, frequency: u32) -> BandplanIter<'_> {
        // if we want a reversible iterator we need to call the full range method
        self.range(frequency..=frequency)
    }

    pub fn range(&self, range: impl RangeBounds<u32>) -> BandplanIter<'_> {
        // we search for both start and end indices, so we can construct a double ended
        // iterator. i think if we don't need this we could just seach for start index
        // and iterate until band.start > end_frequency

        let start_frequency = match range.start_bound() {
            Bound::Included(frequency) => Some(*frequency),
            Bound::Excluded(frequency) => Some(*frequency + 1),
            Bound::Unbounded => None,
        };

        let end_frequency = match range.end_bound() {
            Bound::Included(frequency) => Some(*frequency + 1),
            Bound::Excluded(frequency) => Some(*frequency),
            Bound::Unbounded => None,
        };

        let start_index = start_frequency
            .map(|start_frequency| self.start_index(start_frequency))
            .unwrap_or_default();

        let end_index = end_frequency
            .map(|end_frequency| self.end_index(end_frequency))
            .unwrap_or(self.bands.len());

        let bands = if start_index < end_index {
            &self.bands[start_index..end_index]
        }
        else {
            &[]
        };

        BandplanIter {
            bands: bands.iter(),
            start_frequency,
            end_frequency,
        }
    }

    pub fn international() -> &'static Self {
        static INTERNATIONAL: OnceLock<Bandplan> = OnceLock::new();
        INTERNATIONAL.get_or_init(|| {
            Self::from_reader(BANDPLAN_INTERNATIONAL_BYTES)
                .expect("Failed to parse builtin international bandplan")
        })
    }

    fn start_index(&self, start_frequency: u32) -> usize {
        // we're actually looking for the start index for bands that end just one after
        // the start frequency. this excludes any bands that end on the start frequency
        // (the band end frequency is always exclusive)
        let start_frequency = start_frequency + 1;

        let mut start_index = self
            .by_end
            .binary_search_by_key(&start_frequency, |(band_end, _band_index)| *band_end)
            .unwrap_or_else(identity);

        // if there are multiple bands with this end frequency the binary search will
        // return an arbitrary one, so we just scan back until we find one with
        // a different start frequency.
        // self.by_end is sorted secondarily by index, so the actual band index can only
        // become smaller while doing this. so all bands that end in our frequency range
        // will be included.
        while start_index > 0 && self.by_end[start_index - 1].0 == start_frequency {
            start_index -= 1;
        }

        let start_index = self
            .by_end
            .get(start_index)
            .map(|(_band_end, band_index)| *band_index)
            .unwrap_or(self.bands.len());

        start_index
    }

    fn end_index(&self, end_frequency: u32) -> usize {
        let mut end_index = self
            .bands
            .binary_search_by_key(&end_frequency, |band| band.start)
            .unwrap_or_else(identity);

        // same things as for the start index
        while end_index + 1 < self.bands.len() && self.bands[end_index + 1].end == end_frequency {
            end_index += 1;
        }

        end_index
    }
}

impl FromIterator<Band> for Bandplan {
    fn from_iter<T: IntoIterator<Item = Band>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let (n1, n2) = iter.size_hint();
        let n = n2.unwrap_or(n1);

        let mut bands = Vec::with_capacity(n);
        let mut by_end = Vec::with_capacity(n);

        for (i, band) in iter.enumerate() {
            by_end.push((band.end, i));
            bands.push(band);
        }

        // sort by start frequency
        bands.sort_by_key(|band| band.start);

        // sort by end frequency, then index
        by_end.sort();

        Self { bands, by_end }
    }
}

#[derive(Clone, Debug)]
pub struct BandplanIter<'a> {
    bands: std::slice::Iter<'a, Band>,
    start_frequency: Option<u32>,
    end_frequency: Option<u32>,
}

impl<'a> Iterator for BandplanIter<'a> {
    type Item = &'a Band;

    fn next(&mut self) -> Option<Self::Item> {
        filter_band_iter(
            self.bands.next()?,
            &self.start_frequency,
            &self.end_frequency,
        )
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, n) = self.bands.size_hint();
        (0, n)
    }
}

impl<'a> DoubleEndedIterator for BandplanIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        filter_band_iter(
            self.bands.next_back()?,
            &self.start_frequency,
            &self.end_frequency,
        )
    }
}

#[inline(always)]
fn filter_band_iter<'a>(
    band: &'a Band,
    start_frequency: &Option<u32>,
    end_frequency: &Option<u32>,
) -> Option<&'a Band> {
    (start_frequency.map_or(true, |start_frequency| band.end >= start_frequency)
        && end_frequency.map_or(true, |end_frequency| band.start < end_frequency))
    .then_some(band)
}

#[derive(Clone, Debug, Deserialize)]
pub struct Band {
    pub start: u32,
    pub end: u32,
    pub mode: String,
    pub step: u32,
    #[serde(deserialize_with = "deserialize_color")]
    pub color: Srgba,
    pub name: String,
}

impl Band {
    pub fn contains(&self, frequency: u32) -> bool {
        self.start <= frequency && frequency < self.end
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BandplanWidget<'a> {
    pub bandplan: &'a Bandplan,
    pub view_frequency_band: FrequencyBand,
}

impl<'a> Widget for BandplanWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if area.height > 0 {
            let cells_per_hz = area.width as f32 / self.view_frequency_band.bandwidth() as f32;

            for band in self.bandplan.range(self.view_frequency_band) {
                let cell_start = ((band.start.saturating_sub(self.view_frequency_band.start))
                    as f32
                    * cells_per_hz) as u16;
                let cell_end = ((band.end.min(self.view_frequency_band.end)
                    - self.view_frequency_band.start) as f32
                    * cells_per_hz) as u16;

                for x in cell_start..cell_end {
                    buf[(area.x + x, area.y)].bg = band.color.color.into();
                }

                let text_color = if band.color.color.relative_luminance().luma > 0.5 {
                    Color::Black
                }
                else {
                    Color::White
                };

                buf.set_stringn(
                    area.x + cell_start,
                    area.y,
                    &band.name,
                    (cell_end - cell_start).into(),
                    text_color,
                );
            }
        }
    }
}

fn deserialize_color<'de, D>(deserializer: D) -> Result<Srgba, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
    if s.starts_with('#') {
        if s.len() == 9 {
            let r = u8::from_str_radix(&s[1..3], 16).map_err(serde::de::Error::custom)?;
            let g = u8::from_str_radix(&s[3..5], 16).map_err(serde::de::Error::custom)?;
            let b = u8::from_str_radix(&s[5..7], 16).map_err(serde::de::Error::custom)?;
            let a = u8::from_str_radix(&s[7..9], 16).map_err(serde::de::Error::custom)?;
            Ok(Srgba::new(r, g, b, a).into_format())
        }
        else {
            Err(serde::de::Error::custom(
                "Expected color to be 9 characters long",
            ))
        }
    }
    else {
        Err(serde::de::Error::custom("Expected color to start with '#'"))
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::bandplan::Bandplan;

    #[test]
    fn it_parses_the_builtin_bandplan() {
        let bandplan = Bandplan::international();
        assert!(
            !bandplan.bands.is_empty(),
            "builtin international bandplan is empty"
        );
    }

    #[test]
    fn it_find_the_band_for_a_frequency() {
        let bandplan = Bandplan::international();
        let band = bandplan.get(7_023_567).expect("no band found");
        assert_eq!(band.start, 7_000_000);
        assert_eq!(band.end, 7_080_000);
        assert_eq!(band.mode, "LSB");
        assert_eq!(band.step, 10);
        assert_eq!(band.name, "40m Ham Band|");
    }

    #[test]
    fn it_returns_a_correct_range() {
        let bandplan = Bandplan::international();

        assert_eq!(
            bandplan
                .range(..14123)
                .map(|band| band.start)
                .collect::<Vec<_>>(),
            &[0, 8300, 14000]
        );
        assert_eq!(
            bandplan
                .range(8312..14123)
                .map(|band| band.start)
                .collect::<Vec<_>>(),
            &[8300, 14000]
        );
        assert_eq!(
            bandplan
                .range(2462100000..)
                .map(|band| band.start)
                .collect::<Vec<_>>(),
            &[2462100000, 2473000000]
        );
        assert_eq!(
            bandplan
                .range(8300..=14000)
                .map(|band| band.start)
                .collect::<Vec<_>>(),
            &[8300, 14000]
        );
    }
}
