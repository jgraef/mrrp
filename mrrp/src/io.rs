use std::{
    convert::Infallible,
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytemuck::Pod;
use pin_project_lite::pin_project;

/// Trait for async reading of samples.
///
/// This works pretty much like futures [`AsyncRead`][1],
/// except it works with arbitrary sample types instead of single bytes.
///
/// [1]: https://docs.rs/futures/latest/futures/io/trait.AsyncRead.html
pub trait AsyncReadSamples<S> {
    /// Error that might occur when reading the IQ stream.
    type Error;

    /// Poll the stream to fill a buffer with IQ samples.
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [S],
    ) -> Poll<Result<usize, Self::Error>>;
}

impl<S, T> AsyncReadSamples<S> for &mut T
where
    T: ?Sized + AsyncReadSamples<S> + Unpin,
{
    type Error = <T as AsyncReadSamples<S>>::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [S],
    ) -> Poll<Result<usize, Self::Error>> {
        Pin::new(&mut **self).poll_read_samples(cx, buffer)
    }
}

/// Extension trait for [`AsyncReadSamples`] with some useful methods.
pub trait AsyncReadSamplesExt<S>: AsyncReadSamples<S> {
    /// Read IQ samples into a buffer.
    ///
    /// This will call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] exactly once,
    /// and return the number of bytes read. This is cancellation-safe.
    fn read_samples<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamples<'a, S, Self>
    where
        Self: Unpin,
    {
        ReadSamples {
            inner: self,
            buffer,
        }
    }

    /// Read IQ samples into a buffer until the buffer is full.
    ///
    /// This might call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] multiple
    /// times, and thus is not cancellation-safe.
    fn read_samples_exact<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamplesExact<'a, S, Self>
    where
        Self: Unpin,
    {
        ReadSamplesExact {
            inner: self,
            buffer,
            filled: 0,
        }
    }

    /// Maps any errors returned by the underlying stream with the provided
    /// closure.
    fn map_err<E, F>(self, f: F) -> MapErr<Self, F>
    where
        F: FnMut(Self::Error) -> E,
        Self: Sized,
    {
        MapErr {
            inner: self,
            map_err: f,
        }
    }

    // todo: remove Clone + Default bound, if we switch to using MaybeUnitialized
    fn map<Q, F>(self, f: F, buffer_size: usize) -> Map<Self, S, F>
    where
        S: Clone + Default,
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        Map {
            inner: self,
            map: f,
            buffer: vec![S::default(); buffer_size],
        }
    }

    /// Reads a [`AsyncReadSamples<S>`][AsyncReadSamples] and maps it with the
    /// provided function using the destination buffer.
    ///
    /// # Experimental!
    ///
    /// This uses bytemuck magic to cast the destination buffer to accept `S`
    /// samples and then maps in-place. It is very likely that this still
    /// contains bugs. If you encounter weird behavior, please consider
    /// writing a test for it. And you call always switch to using
    /// [`map`][Self::map] instead.
    fn map_in_place<Q, F>(self, f: F) -> MapInPlace<Self, S, F>
    where
        S: Pod,
        Q: Pod,
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        MapInPlace {
            inner: self,
            map: f,
            _phantom: PhantomData,
        }
    }
}

impl<S, T> AsyncReadSamplesExt<S> for T where T: AsyncReadSamples<S> {}

/// Future that reads samples into a buffer.
pub struct ReadSamples<'a, S, T>
where
    T: ?Sized,
{
    inner: &'a mut T,
    buffer: &'a mut [S],
}

impl<'a, 'b, S, T> Future for ReadSamples<'a, S, T>
where
    T: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<usize, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut *this.inner).poll_read_samples(cx, this.buffer)
    }
}

/// Future that tries to read an exact amount of samples.
#[derive(Debug)]
pub struct ReadSamplesExact<'a, S, T>
where
    T: ?Sized,
{
    inner: &'a mut T,
    buffer: &'a mut [S],
    filled: usize,
}

impl<'a, 'b, S, T> Future for ReadSamplesExact<'a, S, T>
where
    T: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), ReadSamplesExactError<T::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while self.filled < self.buffer.len() {
            let this = &mut *self;
            match Pin::new(&mut *this.inner).poll_read_samples(cx, &mut this.buffer[this.filled..])
            {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(ReadSamplesExactError::Other(error)));
                }
                Poll::Ready(Ok(num_samples_read)) => {
                    if num_samples_read == 0 {
                        break;
                    }
                    else {
                        this.filled += num_samples_read;
                    }
                }
            }
        }

        if self.filled == self.buffer.len() {
            Poll::Ready(Ok(()))
        }
        else {
            Poll::Ready(Err(ReadSamplesExactError::Eof {
                num_samples_read: self.filled,
            }))
        }
    }
}

/// Error returned by
/// [`read_samples_exact`][AsyncReadSamplesExt::read_samples_exact]
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum ReadSamplesExactError<E> {
    /// The stream ended before the buffer could be filled completely.
    #[error("EOF after {num_samples_read} samples")]
    Eof { num_samples_read: usize },

    /// The underlying stream produced an error.
    #[error("{0}")]
    Other(#[from] E),
}

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy, Debug)]
    pub struct MapErr<T, F> {
        #[pin]
        inner: T,
        map_err: F,
    }
}

impl<S, T, E, F> AsyncReadSamples<S> for MapErr<T, F>
where
    T: AsyncReadSamples<S>,
    F: FnMut(T::Error) -> E,
{
    type Error = E;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [S],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();
        this.inner
            .poll_read_samples(cx, buffer)
            .map_err(this.map_err)
    }
}

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Debug)]
    pub struct Map<T, S, F> {
        #[pin]
        inner: T,
        map: F,
        buffer: Vec<S>,
    }
}

impl<S, T, Q, F> AsyncReadSamples<Q> for Map<T, S, F>
where
    S: Pod,
    Q: Pod,
    T: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
    type Error = T::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [Q],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut this = self.project();

        match this.inner.poll_read_samples(cx, &mut this.buffer) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(num_samples_read)) => {
                for i in 0..num_samples_read {
                    buffer[i] = (this.map)(this.buffer[i]);
                }
                Poll::Ready(Ok(num_samples_read))
            }
        }
    }
}

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy, Debug)]
    pub struct MapInPlace<T, S, F> {
        #[pin]
        inner: T,
        map: F,
        _phantom: PhantomData<fn(S)>,
    }
}

impl<S, T, Q, F> AsyncReadSamples<Q> for MapInPlace<T, S, F>
where
    S: Pod,
    Q: Pod,
    T: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
    type Error = T::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [Q],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();
        let num_samples_out = buffer.len();
        const MIN_BUFFER: usize = 32;

        if num_samples_out == 0 {
            Poll::Ready(Ok(0))
        }
        else if num_samples_out < MIN_BUFFER {
            // fall back to using stack-allocated intermediate buffer
            // otherwise a caller like read_exact will provide smaller and smaller buffers,
            // until this can't use it as an intermediate buffer anymore.
            //
            // however this is only a problem if the input samples are larger than the
            // output samples. we do it in either case here though.
            //
            // and furthermore MIN_BUFFER should not be constant, as this edge case really
            // depends on the size difference and alignment. so this needs fixing someway.
            let mut intermediate_buffer = [S::zeroed(); MIN_BUFFER];

            match this
                .inner
                .poll_read_samples(cx, &mut intermediate_buffer[..num_samples_out])
            {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(num_samples_read_in)) => {
                    for i in 0..num_samples_read_in {
                        buffer[i] = (this.map)(intermediate_buffer[i]);
                    }
                    Poll::Ready(Ok(num_samples_read_in))
                }
            }
        }
        else {
            let (_, buffer_in, _) = bytemuck::pod_align_to_mut::<Q, S>(buffer);
            let num_samples_in = buffer_in.len();
            let num_samples = num_samples_out.min(num_samples_in);
            let buffer_in = &mut buffer_in[..num_samples];
            assert!(
                buffer_in.len() > 0,
                "bug: not a single input sample fits into the provided output buffer"
            );

            match this.inner.poll_read_samples(cx, buffer_in) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(num_samples_read_in)) => {
                    if num_samples_out < num_samples_in {
                        // num_samples_out < num_samples_in
                        // sizeof(sample_out) > sizeof(sample_in)
                        // map reverse
                        for i in (0..num_samples_read_in).rev() {
                            let (_, buffer_in, _) = bytemuck::pod_align_to::<Q, S>(buffer);
                            let sample = buffer_in[i];
                            buffer[i] = (this.map)(sample);
                        }
                    }
                    else {
                        // num_samples_out >= num_samples_in
                        // sizeof(sample_out) =< sizeof(sample_in)
                        // map forward
                        for i in 0..num_samples_read_in {
                            let (_, buffer_in, _) = bytemuck::pod_align_to::<Q, S>(buffer);
                            let sample = buffer_in[i];
                            buffer[i] = (this.map)(sample);
                        }
                    }

                    Poll::Ready(Ok(num_samples_read_in))
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Repeat<S> {
    pub sample: S,
}

impl<S> AsyncReadSamples<S> for Repeat<S>
where
    S: Clone,
{
    type Error = Infallible;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut [S],
    ) -> Poll<Result<usize, Self::Error>> {
        buffer
            .iter_mut()
            .for_each(|output| *output = self.sample.clone());
        Poll::Ready(Ok(buffer.len()))
    }
}

pub fn repeat<S>(sample: S) -> Repeat<S> {
    Repeat { sample }
}

#[derive(Clone, Copy, Debug)]
pub struct Cursor<'a, S> {
    pub samples: &'a [S],
    pub position: usize,
}

impl<'a, S> Cursor<'a, S> {
    #[inline(always)]
    pub fn advance(&mut self, amount: usize) {
        self.position += amount;
    }

    #[inline(always)]
    pub fn advance_to_end(&mut self) {
        self.position = self.samples.len();
    }

    #[inline(always)]
    pub fn remaining(&self) -> &[S] {
        &self.samples[self.position..]
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use crate::io::{
        AsyncReadSamplesExt,
        repeat,
    };

    #[test]
    fn it_maps() {
        let input = repeat(12u8);
        let mut output = [0i16; 100];
        input
            .map(|sample| i16::from(sample) * -2, 20)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        assert_eq!(output.len(), 100);
        assert!(output.iter().all(|sample| *sample == -24));
    }

    #[test]
    fn it_maps_in_place_with_same_sample_size() {
        let input = repeat(12u8);
        let mut output = [0; 100];
        input
            .map_in_place(|_sample| -23i8)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        assert_eq!(output.len(), 100);
        assert!(output.iter().all(|sample| *sample == -23));
    }

    #[test]
    fn it_maps_in_place_with_smaller_input_samples() {
        let input = repeat(12u8);
        let mut output = [0; 100];
        input
            .map_in_place(|_sample| -23i16)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        assert_eq!(output.len(), 100);
        assert!(output.iter().all(|sample| *sample == -23));
    }

    #[test]
    fn it_maps_in_place_with_larger_input_samples() {
        let input = repeat(12u16);
        let mut output = [0; 100];
        input
            .map_in_place(|_sample| -23i8)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        assert_eq!(output.len(), 100);
        assert!(output.iter().all(|sample| *sample == -23));
    }
}
