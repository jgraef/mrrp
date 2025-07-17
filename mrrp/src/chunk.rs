use futures_util::Stream;

use crate::buf::SampleBuf;

pub trait ChunkStream<C, S>: Stream<Item = C>
where
    C: SampleBuf<S> + AsRef<[S]>,
{
}
