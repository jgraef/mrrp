use pin_project_lite::pin_project;

pin_project! {
    #[derive(Clone, Debug)]
    pub struct ZipWith<L, R, Sc> {
        left: L,
        right: R,
        scanner: Sc,
    }
}

impl<L, R, Sc> ZipWith<L, R, Sc> {
    #[inline]
    pub fn new(left: L, right: R, scanner: Sc) -> Self {
        Self {
            left,
            right,
            scanner,
        }
    }
}
