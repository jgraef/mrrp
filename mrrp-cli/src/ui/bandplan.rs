use std::{
    borrow::Cow,
    collections::{
        BTreeMap,
        btree_map,
    },
    io::Read,
    ops::{
        Bound,
        RangeBounds,
    },
    sync::OnceLock,
};

use color_eyre::eyre::Error;
use palette::Srgba;
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

// todo: if we don't want to modify the bandplans (which we don't), then we can
// replace the BTreeMaps with sorted Vecs and do binary search. this would also
// allow for a easier implementation of a DoubleEndedIterator
#[derive(Clone, Debug)]
pub struct Bandplan {
    by_start: BTreeMap<u32, usize>,
    by_end: BTreeMap<u32, usize>,
    bands: Vec<Band>,
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
            .collect())
    }

    pub fn push(&mut self, band: Band) {
        let index = self.bands.len();
        self.by_start.insert(band.start, index);
        self.by_end.insert(band.end, index);
        self.bands.push(band);
    }

    pub fn clear(&mut self) {
        self.by_start.clear();
        self.by_end.clear();
        self.bands.clear();
    }

    pub fn get(&self, frequency: u32) -> Option<&Band> {
        let (_end, index) = self
            .by_end
            .range((Bound::Excluded(frequency), Bound::Unbounded))
            .next()?;
        let band = &self.bands[*index];
        (frequency >= band.start).then(|| band)
    }

    pub fn range(&self, range: impl RangeBounds<u32>) -> BandplanIter<'_> {
        fn excluded_bound(bound: Bound<u32>) -> Bound<u32> {
            match bound {
                Bound::Included(start) | Bound::Excluded(start) => Bound::Excluded(start),
                Bound::Unbounded => Bound::Unbounded,
            }
        }

        let iter = self.by_end.range((
            excluded_bound(range.start_bound().cloned()),
            Bound::Unbounded,
        ));
        let end = self
            .by_start
            .range((excluded_bound(range.end_bound().cloned()), Bound::Unbounded))
            .next()
            .map(|(_start, index)| *index);

        BandplanIter {
            iter,
            bands: &self.bands,
            end,
        }
    }

    pub fn international() -> &'static Self {
        static INTERNATIONAL: OnceLock<Bandplan> = OnceLock::new();
        INTERNATIONAL.get_or_init(|| {
            Self::from_reader(&include_bytes!("bandplan.csv")[..])
                .expect("Failed to parse builtin international bandplan")
        })
    }
}

impl FromIterator<Band> for Bandplan {
    fn from_iter<T: IntoIterator<Item = Band>>(iter: T) -> Self {
        let mut by_start = BTreeMap::new();
        let mut by_end = BTreeMap::new();

        let mut i = 0;
        let bands = iter
            .into_iter()
            .inspect(|band| {
                by_start.insert(band.start, i);
                by_end.insert(band.end, i);
                i += 1;
            })
            .collect();

        Self {
            by_start,
            by_end,
            bands,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BandplanIter<'a> {
    iter: btree_map::Range<'a, u32, usize>,
    bands: &'a [Band],
    end: Option<usize>,
}

impl<'a> Iterator for BandplanIter<'a> {
    type Item = &'a Band;

    fn next(&mut self) -> Option<Self::Item> {
        let (_end, index) = self.iter.next()?;
        self.end
            .map_or(true, |end| end != *index)
            .then(|| &self.bands[*index])
    }
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

                buf.set_stringn(
                    area.x + cell_start,
                    area.y,
                    &band.name,
                    (cell_end - cell_start).into(),
                    Color::White,
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
