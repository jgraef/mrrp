#![allow(dead_code)]

use std::{
    borrow::Cow,
    ops::Deref,
    sync::Arc,
};

use parking_lot::RwLock;

use self::inflight::*;

#[derive(Clone, Debug)]
pub struct StagingPool {
    inner: Arc<StagingPoolInner>,
}

#[derive(Debug)]
struct StagingPoolInner {
    chunk_label: Cow<'static, str>,
    state: RwLock<StagingPoolState>,
}

#[derive(Debug)]
struct StagingPoolState {
    /// Minimum size of an individual chunk
    chunk_size: ChunkSize,

    /// Chunks that are back from the GPU and ready to be mapped for write and
    /// put into `active_chunks`.
    free_chunks: Vec<Chunk>,

    /// How many chunks are currently not mapped
    in_flight_count: usize,

    /// Total number of allocated chunks
    total_allocated_count: usize,

    /// Total allocated size in bytes
    total_allocated_bytes: u64,

    /// Total staged (in-flight) size in bytes
    total_staged_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkSize {
    pub chunk_size: u64,
    pub adaptive: bool,
}

impl ChunkSize {
    pub fn get(&mut self, required: u64) -> u64 {
        if required > self.chunk_size {
            if self.adaptive {
                self.chunk_size = (2 * self.chunk_size).min(required);
                self.chunk_size
            }
            else {
                required
            }
        }
        else {
            self.chunk_size
        }
    }
}

impl StagingPool {
    pub fn new(chunk_size: ChunkSize, chunk_label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            inner: Arc::new(StagingPoolInner {
                chunk_label: chunk_label.into(),
                state: RwLock::new(StagingPoolState {
                    chunk_size,
                    free_chunks: vec![],
                    in_flight_count: 0,
                    total_allocated_count: 0,
                    total_allocated_bytes: 0,
                    total_staged_bytes: 0,
                }),
            }),
        }
    }

    #[must_use]
    pub fn begin(&self) -> StagingTransaction {
        StagingTransaction::from_pool(self.clone())
    }

    pub fn info(&self) -> StagingPoolInfo {
        let state = self.inner.state.read();
        StagingPoolInfo {
            in_flight_count: state.in_flight_count,
            free_count: state.free_chunks.len(),
            total_allocation_count: state.total_allocated_count,
            total_allocation_bytes: state.total_allocated_bytes,
            total_staged_bytes: state.total_staged_bytes,
        }
    }
}

#[derive(Debug)]
pub struct StagingTransaction {
    pool: StagingPool,

    /// Chunks into which we are accumulating data to be transferred.
    ///
    /// Note: if the WriteStagingBelt is dropped while it has active chunks
    /// (i.e. finish wasn't called), the chunks will not be reused.
    active_chunks: Vec<Chunk>,
}

impl StagingTransaction {
    fn from_pool(pool: StagingPool) -> Self {
        Self {
            pool,
            active_chunks: vec![],
        }
    }

    fn discard_impl(&mut self) {
        let mut state = self.pool.inner.state.write();
        state.in_flight_count -= self.active_chunks.len();
        state
            .free_chunks
            .extend(self.active_chunks.drain(..).map(|mut chunk| {
                chunk.reset();
                chunk
            }));
    }

    pub fn allocate<R>(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::BufferSize,
        alignment: wgpu::BufferSize,
        f: impl FnOnce(wgpu::BufferSlice<'_>) -> R,
    ) -> R {
        let chunk_index = self
            .active_chunks
            .iter()
            .position(|chunk| chunk.can_allocate(size, alignment.get()))
            .unwrap_or_else(|| {
                let mut state = self.pool.inner.state.write();
                state.in_flight_count += 1;

                let chunk = if let Some(index) = state
                    .free_chunks
                    .iter()
                    .position(|chunk| chunk.can_allocate(size, alignment.get()))
                {
                    state.free_chunks.swap_remove(index)
                }
                else {
                    let size = state.chunk_size.get(size.get());
                    state.total_allocated_count += 1;
                    state.total_allocated_bytes += size;
                    drop(state);

                    tracing::debug!(?size, "allocating staging buffer");

                    Chunk {
                        buffer: device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some(&self.pool.inner.chunk_label),
                            size,
                            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                            mapped_at_creation: true,
                        }),
                        offset: 0,
                    }
                };

                let chunk_index = self.active_chunks.len();
                self.active_chunks.push(chunk);
                chunk_index
            });

        let chunk = &mut self.active_chunks[chunk_index];
        let allocation_offset = chunk.allocate(size, alignment.get());

        let staging_buffer_slice = chunk
            .buffer
            .slice(allocation_offset..allocation_offset + size.get());

        f(staging_buffer_slice)
    }

    pub fn commit(mut self, command_encoder: &mut wgpu::CommandEncoder) {
        for chunk in &self.active_chunks {
            chunk.buffer.unmap();
        }

        let inflight_chunks =
            InflightChunks::new(self.pool.clone(), std::mem::take(&mut self.active_chunks));

        command_encoder.on_submitted_work_done(move || {
            // the command encoder got submitted and is done, we can recall the chunks
            inflight_chunks.recall();
        });
    }

    pub fn discard(mut self) {
        if !self.active_chunks.is_empty() {
            self.discard_impl()
        }
    }

    /// Allocates a slice in a staging buffer, but doesn't record any copy
    /// commands.
    ///
    /// You can record your own copy commands using the provided
    /// `with_buffer_slice` closure.
    ///
    /// If you want to get a [`BufferViewMut`](wgpu::BufferViewMut) and have it
    /// automatically be copied to your destination buffer, use
    /// [`write_buffer`](Self::write_buffer) instead.
    #[must_use]
    pub fn view_mut(
        &mut self,
        size: wgpu::BufferSize,
        alignment: wgpu::BufferSize,
        with_buffer_slice: impl FnOnce(wgpu::BufferSlice),
        device: &wgpu::Device,
    ) -> wgpu::BufferViewMut {
        assert!(
            size.get().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "WriteStagingBelt allocation size {size} must be a multiple of `COPY_BUFFER_ALIGNMENT`"
        );
        assert!(
            alignment.get().is_power_of_two(),
            "alignment must be a power of two, not {alignment}"
        );

        // At minimum, we must have alignment sufficient to map the buffer.
        let alignment = alignment.max(wgpu::BufferSize::new(wgpu::MAP_ALIGNMENT).unwrap());

        self.allocate(device, size, alignment, |staging_buffer_slice| {
            with_buffer_slice(staging_buffer_slice);
            staging_buffer_slice.get_mapped_range_mut()
        })
    }

    #[must_use]
    pub fn write_buffer(
        &mut self,
        destination: wgpu::BufferSlice,
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::BufferViewMut {
        let offset = destination.offset();
        let size = destination.size();

        assert!(
            size.get().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "allocation size {size} must be a multiple of `COPY_BUFFER_ALIGNMENT`"
        );
        assert!(
            offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "WriteStaging offset {offset} must be a multiple of `COPY_BUFFER_ALIGNMENT`"
        );

        self.view_mut(
            size,
            wgpu::BufferSize::new(wgpu::COPY_BUFFER_ALIGNMENT).unwrap(),
            |staging_buffer_slice| {
                command_encoder.copy_buffer_to_buffer(
                    staging_buffer_slice.buffer(),
                    staging_buffer_slice.offset(),
                    destination.buffer(),
                    offset,
                    size.get(),
                );
            },
            device,
        )
    }

    pub fn write_buffer_from_slice(
        &mut self,
        destination: wgpu::BufferSlice,
        data: &[u8],
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
    ) {
        assert_eq!(destination.size().get(), data.len() as wgpu::BufferAddress);
        let mut view = self.write_buffer(destination, device, command_encoder);
        view.copy_from_slice(data);
    }
}

impl Drop for StagingTransaction {
    fn drop(&mut self) {
        if !self.active_chunks.is_empty() {
            tracing::warn!("WriteStagingBelt not committed. Staging buffers will not be mapped.");
            self.discard_impl();
        }
    }
}

/// Helpers to make sure in-flight chunks are always accounted for.
///
/// This basically wraps them and handles the case if they're dropped somewhere.
mod inflight {
    use super::*;

    // when we recall the chunks and map them, we need to move them individually
    // into the map_async callback with a pool anyway. so we pair them up
    // now.
    pub(super) struct InflightChunk {
        inner: Option<(StagingPool, Chunk)>,
    }

    // then we give them a Drop impl to make sure they're always accounted for
    impl Drop for InflightChunk {
        fn drop(&mut self) {
            if let Some((pool, chunk)) = self.inner.take() {
                // this chunk got lost somewhere (map_sync dropped it). we'll drop it because we
                // don't know its state (whether it's mapped or not). but we want to take it
                // into account
                tracing::warn!(?chunk, "inflight chunk dropped");
                let mut state = pool.inner.state.write();
                state.in_flight_count -= 1;
            }
        }
    }

    impl Deref for InflightChunk {
        type Target = Chunk;

        fn deref(&self) -> &Self::Target {
            // this is always okay, because we only take out the chunk when we take
            // ownership of this.
            &self.inner.as_ref().unwrap().1
        }
    }

    impl InflightChunk {
        pub fn new(pool: StagingPool, chunk: Chunk) -> Self {
            Self {
                inner: Some((pool, chunk)),
            }
        }
        pub fn into_inner(mut self) -> (StagingPool, Chunk) {
            self.inner.take().unwrap()
        }
    }

    // this will hold all the inflight chunks for the on_submitted_work_done
    // callback. if the user drops the command encoder this will be dropped,
    // and we can safely recall the chunks
    pub(super) struct InflightChunks {
        pool: StagingPool,
        chunks: Vec<Chunk>,
    }

    impl InflightChunks {
        pub fn new(pool: StagingPool, chunks: Vec<Chunk>) -> Self {
            Self { pool, chunks }
        }
    }

    impl InflightChunks {
        pub fn recall(mut self) {
            // we could just drop it, since the drop impl will call the same method, but
            // this is more explicit.
            self.recall_impl();
        }

        fn recall_impl(&mut self) {
            for chunk in self.chunks.drain(..) {
                let buffer = chunk.buffer.clone();

                let chunk = InflightChunk::new(self.pool.clone(), chunk);

                buffer.map_async(wgpu::MapMode::Write, .., move |result| {
                    if let Err(error) = result {
                        tracing::error!("{error}");
                    }
                    else {
                        // take out the chunk from the `InflightChunk`, so it's Drop doesn't do
                        // anything
                        let (pool, mut chunk) = chunk.into_inner();

                        // well, this includes alignment, but it's only used for debug info :shrug:
                        let allocated = chunk.offset;

                        chunk.reset();

                        // take account and put back into free list
                        let mut state = pool.inner.state.write();
                        state.in_flight_count -= 1;
                        state.total_staged_bytes += allocated;
                        state.free_chunks.push(chunk);
                    }
                });
            }
        }
    }

    impl Drop for InflightChunks {
        fn drop(&mut self) {
            // this is to make sure active buffers are recalled even if the command encoder
            // is dropped and never submitted
            self.recall_impl();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StagingPoolInfo {
    pub in_flight_count: usize,
    pub free_count: usize,
    pub total_allocation_count: usize,
    pub total_allocation_bytes: u64,
    pub total_staged_bytes: u64,
}

#[derive(Debug)]
struct Chunk {
    buffer: wgpu::Buffer,
    offset: wgpu::BufferAddress,
}

impl Chunk {
    fn can_allocate(&self, size: wgpu::BufferSize, alignment: wgpu::BufferAddress) -> bool {
        let alloc_start = wgpu::util::align_to(self.offset, alignment);
        let alloc_end = alloc_start + size.get();

        alloc_end <= self.buffer.size()
    }

    fn allocate(
        &mut self,
        size: wgpu::BufferSize,
        alignment: wgpu::BufferAddress,
    ) -> wgpu::BufferAddress {
        let alloc_start = wgpu::util::align_to(self.offset, alignment);
        let alloc_end = alloc_start + size.get();

        assert!(alloc_end <= self.buffer.size());
        self.offset = alloc_end;
        alloc_start
    }

    fn reset(&mut self) {
        self.offset = 0;
    }
}
