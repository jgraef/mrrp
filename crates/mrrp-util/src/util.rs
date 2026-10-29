use std::fmt::Debug;

pub struct SkipDebug<'a, T> {
    value: &'a T,
    place_holder: &'a str,
}

impl<'a, T> SkipDebug<'a, T> {
    #[allow(dead_code)]
    #[inline]
    pub fn new(value: &'a T) -> Self {
        Self::with_place_holder(value, "<skipped>")
    }

    #[inline]
    pub fn closure(value: &'a T) -> Self {
        Self::with_place_holder(value, "<closure>")
    }

    #[inline]
    pub fn with_place_holder(value: &'a T, place_holder: &'a str) -> Self {
        Self {
            value,
            place_holder,
        }
    }
}

impl<'a, T> Debug for SkipDebug<'a, T> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = &self.value;
        write!(f, "{}", self.place_holder)
    }
}
