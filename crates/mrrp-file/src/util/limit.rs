use std::io::{
    Read,
    Seek,
    SeekFrom,
};

#[derive(Clone, Copy, Debug)]
pub struct LimitReader<R> {
    inner: R,
    offset: u64,
    size: u64,
    position: u64,
}

impl<R> LimitReader<R> {
    /// Create new limited reader.
    ///
    /// This assumes the reader is at the start of limit, i.e. at `offset`. Use
    /// [`new_seek`](Self::new_seek) if you want to automatically seek to the
    /// start of the limits.
    pub fn new(inner: R, offset: u64, size: u64) -> Self {
        Self {
            inner,
            offset,
            size,
            position: offset,
        }
    }
}

impl<R> LimitReader<R>
where
    R: Seek,
{
    pub fn new_seek(mut inner: R, offset: u64, size: u64) -> Result<Self, std::io::Error> {
        inner.seek(SeekFrom::Start(offset))?;
        Ok(Self::new(inner, offset, size))
    }
}

impl<R> Read for LimitReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let end = self.offset + self.size;
        let remaining = end.saturating_sub(self.position);

        let n_limited = buf.len().min(remaining.try_into().unwrap_or(usize::MAX));
        let n_read = self.inner.read(&mut buf[..n_limited])?;

        self.position += u64::try_from(n_read).unwrap();
        assert!(self.position <= self.offset + self.size);

        Ok(n_read)
    }
}

impl<R> Seek for LimitReader<R>
where
    R: Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        // check if the position we track is the same as the one in the
        // underlying stream.
        debug_assert_eq!(self.inner.stream_position()?, self.position);

        let position = match pos {
            SeekFrom::Start(offset) => Some(self.offset + offset),
            SeekFrom::End(offset) => (self.offset + self.size).checked_add_signed(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
        };

        if let Some(position) = position
            && position >= self.offset
            && position < self.offset + self.size
        {
            self.position = position;
            self.inner.seek(SeekFrom::Start(position))
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    #[inline(always)]
    fn stream_position(&mut self) -> std::io::Result<u64> {
        Ok(self.position)
    }
}
