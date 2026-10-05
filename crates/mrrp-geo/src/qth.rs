//! [Maidenhead locator][1], a.k.a QTH code
//!
//! [1]: https://en.wikipedia.org/wiki/Maidenhead_Locator_System

use std::{
    fmt::{
        Debug,
        Display,
        Write,
    },
    ops::RangeInclusive,
    str::{
        Chars,
        FromStr,
    },
};

use crate::HorizontalGeodetic;

pub const FIELD_CHARS: RangeInclusive<u8> = b'A'..=b'R';
pub const SQUARE_CHARS: RangeInclusive<u8> = b'0'..=b'9';
pub const SUBSQUARE_CHARS: RangeInclusive<u8> = b'A'..=b'X';
pub const EXTENDED_SQUARE_CHARS: RangeInclusive<u8> = b'0'..=b'9';

pub const FIELD_DIVISIONS: u8 = 18;
pub const SQUARE_DIVISIONS: u8 = 10;
pub const SUBSQUARE_DIVISIONS: u8 = 24;
pub const EXTENDED_SQUARE_DIVISIONS: u8 = 10;

pub const FIELD_EXTENT: HorizontalGeodetic = HorizontalGeodetic {
    longitude: 360.0 / FIELD_DIVISIONS as f64,
    latitude: 180.0 / FIELD_DIVISIONS as f64,
};
pub const SQUARE_EXTENT: HorizontalGeodetic = HorizontalGeodetic {
    longitude: FIELD_EXTENT.longitude / SQUARE_DIVISIONS as f64,
    latitude: FIELD_EXTENT.latitude / SQUARE_DIVISIONS as f64,
};
pub const SUBSQUARE_EXTENT: HorizontalGeodetic = HorizontalGeodetic {
    longitude: SQUARE_EXTENT.longitude / SUBSQUARE_DIVISIONS as f64,
    latitude: SQUARE_EXTENT.latitude / SUBSQUARE_DIVISIONS as f64,
};
pub const EXTENDED_SQUARE_EXTENT: HorizontalGeodetic = HorizontalGeodetic {
    longitude: SQUARE_EXTENT.longitude / EXTENDED_SQUARE_DIVISIONS as f64,
    latitude: SQUARE_EXTENT.latitude / EXTENDED_SQUARE_DIVISIONS as f64,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Qth {
    field: Pair,
    square: Pair,
    subsquare: Pair,
    extended_square: Option<Pair>,
}

impl Qth {
    pub fn from_geodetic(mut geodetic: HorizontalGeodetic, extended: bool) -> Self {
        // false easting
        geodetic.longitude = (geodetic.longitude + 180.0).rem_euclid(360.0);
        // false northing
        geodetic.latitude += 90.0;

        // normalize to [0, 1]
        geodetic.longitude /= 360.0;
        geodetic.latitude /= 180.0;

        #[inline(always)]
        fn subdivide(x: &mut f64, divisions: u8) -> u8 {
            *x *= divisions as f64;
            let y = *x as u8;
            *x = x.fract();
            y
        }

        let field = Pair {
            longitude: subdivide(&mut geodetic.longitude, FIELD_DIVISIONS),
            latitude: subdivide(&mut geodetic.latitude, FIELD_DIVISIONS),
        };

        let square = Pair {
            longitude: subdivide(&mut geodetic.longitude, SQUARE_DIVISIONS),
            latitude: subdivide(&mut geodetic.latitude, SQUARE_DIVISIONS),
        };

        let subsquare = Pair {
            longitude: subdivide(&mut geodetic.longitude, SUBSQUARE_DIVISIONS),
            latitude: subdivide(&mut geodetic.latitude, SUBSQUARE_DIVISIONS),
        };

        let extended_square = extended.then(|| {
            Pair {
                longitude: subdivide(&mut geodetic.longitude, EXTENDED_SQUARE_DIVISIONS),
                latitude: subdivide(&mut geodetic.latitude, EXTENDED_SQUARE_DIVISIONS),
            }
        });

        Self {
            field,
            square,
            subsquare,
            extended_square,
        }
    }

    pub fn geodetic(&self) -> HorizontalGeodetic {
        let mut geodetic = HorizontalGeodetic {
            latitude: 0.5,
            longitude: 0.5,
        };

        fn unsubdivide(x: &mut f64, y: u8, divisions: u8) {
            *x += y as f64;
            *x /= divisions as f64;
        }

        if let Some(extended_square) = self.extended_square {
            unsubdivide(
                &mut geodetic.longitude,
                extended_square.longitude,
                EXTENDED_SQUARE_DIVISIONS,
            );
            unsubdivide(
                &mut geodetic.latitude,
                extended_square.latitude,
                EXTENDED_SQUARE_DIVISIONS,
            );
        }

        unsubdivide(
            &mut geodetic.longitude,
            self.subsquare.longitude,
            SUBSQUARE_DIVISIONS,
        );

        unsubdivide(
            &mut geodetic.latitude,
            self.subsquare.latitude,
            SUBSQUARE_DIVISIONS,
        );

        unsubdivide(
            &mut geodetic.longitude,
            self.square.longitude,
            SQUARE_DIVISIONS,
        );
        unsubdivide(
            &mut geodetic.latitude,
            self.square.latitude,
            SQUARE_DIVISIONS,
        );

        unsubdivide(
            &mut geodetic.longitude,
            self.field.longitude,
            FIELD_DIVISIONS,
        );
        unsubdivide(&mut geodetic.latitude, self.field.latitude, FIELD_DIVISIONS);

        // longitude and latitude are now normalized in [0, 1]

        // map to false easted/northed degrees
        geodetic.longitude *= 360.0;
        geodetic.latitude *= 180.0;

        // undo false easting
        geodetic.longitude -= 180.0;
        // undo false northing
        geodetic.latitude -= 90.0;

        // the decoded coordinates now point to the south-west point of the
        // square. we want to point to the center, so we need to add half the
        // extent of a subsquare or extended square.
        /*if self.extended_square.is_some() {
            geodetic.longitude += EXTENDED_SQUARE_EXTENT.longitude;
            geodetic.latitude += EXTENDED_SQUARE_EXTENT.latitude;
        }*/

        geodetic
    }

    /// Helper that doesn't rely on std
    ///
    /// Can be used to format into a custom buffer or something. We wanted to
    /// use this for serde::Serialize, but would need a stack-allocated string
    /// buffer.
    fn format<E>(&self, mut out: impl FnMut(char) -> Result<(), E>) -> Result<(), E> {
        fmt_field_component(&mut out, self.field.longitude, FIELD_CHARS)?;
        fmt_field_component(&mut out, self.field.latitude, FIELD_CHARS)?;

        fmt_field_component(&mut out, self.square.longitude, SQUARE_CHARS)?;
        fmt_field_component(&mut out, self.square.latitude, SQUARE_CHARS)?;

        fmt_field_component(&mut out, self.subsquare.longitude, SUBSQUARE_CHARS)?;
        fmt_field_component(&mut out, self.subsquare.latitude, SUBSQUARE_CHARS)?;

        if let Some(extended_square) = self.extended_square {
            fmt_field_component(&mut out, extended_square.longitude, EXTENDED_SQUARE_CHARS)?;
            fmt_field_component(&mut out, extended_square.latitude, EXTENDED_SQUARE_CHARS)?;
        }

        Ok(())
    }
}

impl Display for Qth {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.format(move |c| f.write_char(c))
    }
}

impl Debug for Qth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // helper to format the field using Display. feature
        // `debug_closure_helpers` will make this easier.

        struct AsDisplay<'a>(&'a Qth);
        impl<'a> Debug for AsDisplay<'a> {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                Display::fmt(self.0, f)
            }
        }

        f.debug_tuple("Qth").field(&AsDisplay(self)).finish()
    }
}

impl FromStr for Qth {
    type Err = QthParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parser = Parser::new(s);
        let field = parser.parse_field_pair()?;
        let square = parser.parse_square_pair()?;
        let subsquare = parser.parse_subsquare_pair()?;
        let ext = parser.parse_extended_square_pair()?;

        Ok(Self {
            field,
            square,
            subsquare,
            extended_square: ext,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid QTH code: {input}")]
pub struct QthParseError {
    pub input: String,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Pair {
    pub longitude: u8,
    pub latitude: u8,
}

#[inline]
fn fmt_field_component<E>(
    mut out: impl FnMut(char) -> Result<(), E>,
    c: u8,
    chars: RangeInclusive<u8>,
) -> Result<(), E> {
    let c: char = (c + chars.start()).into();
    out(c)
}

macro_rules! try_opt {
    ($result_opt:expr) => {
        match $result_opt {
            Ok(Some(value)) => value,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        }
    };
}

// note: if we want to make this pub, we need to consider what part of `Pair`
// needs to be pub. `Pair` has invariants, so the fields can't just be pub.
#[derive(Clone, Debug)]
struct Parser<'a> {
    input: &'a str,
    chars: Chars<'a>,
}

impl<'a> Parser<'a> {
    #[inline]
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
        }
    }

    #[inline]
    fn err(&self) -> QthParseError {
        QthParseError {
            input: self.input.to_owned(),
        }
    }

    #[inline]
    pub fn next_char(&mut self) -> Result<Option<u8>, QthParseError> {
        self.chars
            .next()
            .map(|c| u8::try_from(c).map_err(|_| self.err()))
            .transpose()
    }

    #[inline]
    pub fn next_letter(&mut self) -> Result<Option<u8>, QthParseError> {
        Ok(self.next_char()?.map(|c| c.to_ascii_uppercase()))
    }

    #[inline]
    pub fn next_field_component(&mut self) -> Result<u8, QthParseError> {
        let c = self.next_letter()?.ok_or_else(|| self.err())?;
        self.map_char(c, FIELD_CHARS)
    }

    #[inline]
    pub fn next_square_component(&mut self) -> Result<u8, QthParseError> {
        let c = self.next_letter()?.ok_or_else(|| self.err())?;
        self.map_char(c, SQUARE_CHARS)
    }

    #[inline]
    pub fn next_subsquare_component(&mut self) -> Result<u8, QthParseError> {
        let c = self.next_letter()?.ok_or_else(|| self.err())?;
        Ok(self.map_char(c, SUBSQUARE_CHARS)?)
    }

    #[inline]
    pub fn next_extended_square_component(&mut self) -> Result<Option<u8>, QthParseError> {
        let c = try_opt!(self.next_letter());
        Ok(Some(self.map_char(c, EXTENDED_SQUARE_CHARS)?))
    }

    #[inline]
    fn map_char(&self, c: u8, chars: RangeInclusive<u8>) -> Result<u8, QthParseError> {
        if chars.contains(&c) {
            Ok(c - chars.start())
        }
        else {
            return Err(self.err());
        }
    }

    #[inline]
    pub fn parse_field_pair(&mut self) -> Result<Pair, QthParseError> {
        let longitude = self.next_field_component()?;
        let latitude = self.next_field_component()?;
        Ok(Pair {
            longitude,
            latitude,
        })
    }

    #[inline]
    pub fn parse_square_pair(&mut self) -> Result<Pair, QthParseError> {
        let longitude = self.next_square_component()?;
        let latitude = self.next_square_component()?;
        Ok(Pair {
            longitude,
            latitude,
        })
    }

    #[inline]
    pub fn parse_subsquare_pair(&mut self) -> Result<Pair, QthParseError> {
        let longitude = self.next_subsquare_component()?;
        let latitude = self.next_subsquare_component()?;
        Ok(Pair {
            longitude,
            latitude,
        })
    }

    #[inline]
    pub fn parse_extended_square_pair(&mut self) -> Result<Option<Pair>, QthParseError> {
        let longitude = try_opt!(self.next_extended_square_component());
        let latitude = try_opt!(self.next_extended_square_component());
        Ok(Some(Pair {
            longitude,
            latitude,
        }))
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use std::borrow::Cow;

    use serde::{
        Deserialize,
        Serialize,
    };

    use crate::Qth;

    impl Serialize for Qth {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            // todo: use format helper to do without allocation, but we need a
            // stack-allocated string buffer.
            let s = self.to_string();
            serializer.serialize_str(&s)
        }
    }

    impl<'de> Deserialize<'de> for Qth {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
            s.parse().map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use crate::{
        HorizontalGeodetic,
        Qth,
    };

    #[test]
    fn qth_parse() {
        let qth: Qth = "JN18DU".parse().unwrap();
        assert_eq!(qth.field.longitude, 9);
        assert_eq!(qth.field.latitude, 13);
        assert_eq!(qth.square.longitude, 1);
        assert_eq!(qth.square.latitude, 8);
        assert_eq!(qth.subsquare.longitude, 3);
        assert_eq!(qth.subsquare.latitude, 20);
    }

    #[test]
    fn qth_from_geodetic() {
        let qth = Qth::from_geodetic(
            HorizontalGeodetic {
                latitude: 48.856076,
                longitude: 2.295486,
            },
            false,
        );
        assert_eq!(qth, "JN18DU".parse().unwrap());
    }

    #[test]
    fn qth_to_geodetic() {
        // reference from www.giangrandi.org/electronics/radio/qthloccalc.shtml

        let qth: Qth = "JN18DU".parse().unwrap();
        let geodetic = qth.geodetic();

        assert_abs_diff_eq!(geodetic.latitude, 48.854, epsilon = 0.001);
        assert_abs_diff_eq!(geodetic.longitude, 2.292, epsilon = 0.001);
    }
}
