use std::{
    collections::VecDeque,
    sync::Arc,
};

use bytemuck::{
    Pod,
    Zeroable,
};
use egui::{
    Color32,
    Vec2,
};
use parking_lot::{
    RwLock,
    RwLockWriteGuard,
};
use wgpu::util::DeviceExt;

use crate::{
    GetWidgetRenderState,
    util::{
        color32_to_linrgba,
        ring_buffer::{
            Range,
            RingBufferAllocator,
            Slice,
        },
        staging::{
            ChunkSize,
            StagingPool,
            StagingTransaction,
        },
    },
};

#[derive(Debug)]
pub struct WaterfallView<'a> {
    state: &'a WaterfallState,
    desired_size: Vec2,
    style: WaterfallStyle,
    min_db: f32,
    max_db: f32,
    start_frequency: f32,
    end_frequency: f32,
}

impl<'a> WaterfallView<'a> {
    pub fn new(state: &'a WaterfallState) -> Self {
        Self {
            state,
            desired_size: Vec2::INFINITY,
            style: Default::default(),
            min_db: -100.0,
            max_db: 0.0,
            start_frequency: 0.0,
            end_frequency: 1000000.0,
        }
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

    pub fn frequency_range(mut self, start_frequency: f32, end_frequency: f32) -> Self {
        self.start_frequency = start_frequency;
        self.end_frequency = end_frequency;
        self
    }

    pub fn style(mut self, style: WaterfallStyle) -> Self {
        self.style = style;
        self
    }
}

impl<'a> egui::Widget for WaterfallView<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.allocate_response(
            ui.available_size(),
            egui::Sense::HOVER | egui::Sense::CLICK | egui::Sense::DRAG,
        );

        if !ui.is_sizing_pass() && ui.is_rect_visible(response.rect) {
            let num_lines = response.rect.height() as usize;

            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                response.rect,
                PaintCallback {
                    shared_state: self.state.shared_state.clone(),
                    config: ConfigData::new(
                        &self.style,
                        self.min_db,
                        self.max_db,
                        self.start_frequency,
                        self.end_frequency,
                    ),
                    num_lines,
                },
            ));
        }

        response
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaterfallState {
    shared_state: Arc<RwLock<State>>,
}

impl WaterfallState {
    pub fn update(&self) -> WaterfallStateUpdateGuard<'_> {
        let state = self.shared_state.write();
        WaterfallStateUpdateGuard { state }
    }
}

#[derive(Debug)]
pub struct WaterfallStateUpdateGuard<'a> {
    state: RwLockWriteGuard<'a, State>,
}

impl<'a> WaterfallStateUpdateGuard<'a> {
    pub fn push(&mut self, line: WaterfallLine) {
        self.state.queued_lines.push(line);
    }
}

impl<'a> Drop for WaterfallStateUpdateGuard<'a> {
    fn drop(&mut self) {
        // we don't want to flush on every line if possible, as it can be more efficient
        // if data transfers can be batched. thus we prefer flushing this queue when
        // rendering happens. but we also can't have this grow limitless. also the
        // longer we delay a flush the larger/more staging buffers we need.

        if self.state.queued_lines.len() >= 32 {
            self.state.flush_background();

            // normally flush_background should clear the queue, but it might not if it
            // doesn't have a device/queue or can't create a staging transaction. but we
            // absolutely don't want this queue to grow without bounds.
            self.state.queued_lines.clear();
        }
    }
}

#[derive(Clone, Debug)]
pub struct WaterfallStyle {
    pub background_color: Color32,
    pub foreground_color1: Color32,
    pub foreground_color2: Color32,
}

impl Default for WaterfallStyle {
    fn default() -> Self {
        Self {
            background_color: Color32::TRANSPARENT,
            foreground_color1: Color32::from_rgba_unmultiplied(200, 0, 200, 255),
            foreground_color2: Color32::from_rgba_unmultiplied(64, 0, 64, 255),
        }
    }
}

#[derive(Debug)]
struct PaintCallback {
    shared_state: Arc<RwLock<State>>,
    config: ConfigData,
    num_lines: usize,
}

impl egui_wgpu::CallbackTrait for PaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // create the render pipeline, if it doesn't exist
        let render_state = callback_resources.expect_widget_render_state();
        let pipeline = callback_resources
            .entry()
            .or_insert_with(|| Pipeline::new(device, render_state.target_texture_format));

        let mut state = self.shared_state.write();

        // we need to remember the device and queue so we can background flush
        if state.device_and_queue.is_none() {
            state.device_and_queue = Some((device.clone(), queue.clone()));
        }

        // stream data to GPU
        state.flush(
            device,
            egui_encoder,
            &pipeline,
            &self.config,
            self.num_lines,
        );

        vec![]
    }

    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // todo
        vec![]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let pipeline = callback_resources.get::<Pipeline>().expect("pipeline");

        let state = self.shared_state.read();

        if let Some(bind_group) = &state.bind_group {
            render_pass.set_pipeline(&pipeline.pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

#[derive(Debug)]
struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, target_texture_format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waterfall"),
            entries: &[
                // config
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // index
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // data
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waterfall"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("waterfall.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("waterfall"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_texture_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    /// The config that is currently stored in the GPU buffer
    config: Option<ConfigData>,

    /// The GPU buffer storing the config
    config_buffer: Option<wgpu::Buffer>,

    /// Lines queued on the CPU-side
    queued_lines: Vec<WaterfallLine>,

    // Ring buffer holding the lines
    ring_buffer: RingBuffer,

    /// Bind group of config and data buffer
    bind_group: Option<wgpu::BindGroup>,

    /// pool of staging buffers
    staging_pool: Option<StagingPool>,

    /// device and queue in case we need to flush buffers without the UI
    /// rendering
    device_and_queue: Option<(wgpu::Device, wgpu::Queue)>,
}

impl State {
    fn flush_background(&mut self) {
        // get the device
        let Some((device, queue)) = &self.device_and_queue
        else {
            return;
        };

        // begin staging transaction
        let Some(mut staging) =
            begin_staging_transaction(&mut self.staging_pool, &self.queued_lines)
        else {
            return;
        };

        //tracing::debug!(count = self.queued_lines.len(), "background flush");

        // we need to create our own command encoder
        let mut command_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("waterfall background flush"),
        });

        // flush new lines to gpu
        let buffers_reallocated = self.ring_buffer.push_back(
            &self.queued_lines,
            device,
            &mut command_encoder,
            &mut staging,
        );
        self.queued_lines.clear();

        // if buffers were reallocated we'll just remove the bind group. a visible flush
        // can later create them
        if buffers_reallocated {
            self.bind_group = None;
        }

        // commit staging transaction
        staging.commit(&mut command_encoder);

        // submit command encoder
        // fixme: this blocks when window is in background (https://github.com/jgraef/mrrp/issues/1)
        queue.submit([command_encoder.finish()]);

        //tracing::debug!("background flush complete");
    }

    fn flush(
        &mut self,
        device: &wgpu::Device,
        mut command_encoder: &mut wgpu::CommandEncoder,
        pipeline: &Pipeline,
        config: &ConfigData,
        num_lines: usize,
    ) {
        // begin staging transaction
        let Some(mut staging) =
            begin_staging_transaction(&mut self.staging_pool, &self.queued_lines)
        else {
            return;
        };

        // update config buffer

        let config_buffer = self.config_buffer.get_or_insert_with(|| {
            tracing::debug!("creating waterfall config buffer");

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("waterfall config"),
                contents: bytemuck::bytes_of(config),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            });

            // note: no need to recreate a bind group here, since we can't have one already
            // anyway
            assert!(self.bind_group.is_none());

            self.config = Some(*config);

            buffer
        });

        if let Some(current_config) = &self.config
            && current_config != config
        {
            tracing::debug!("writing waterfall config buffer");

            let config_bytes = bytemuck::bytes_of(config);
            staging.write_buffer_from_slice(
                config_buffer.slice(..),
                config_bytes,
                &device,
                &mut command_encoder,
            );

            self.config = Some(*config);
        }

        // update the line capacity
        self.ring_buffer.set_line_capacity(num_lines);

        // flush new lines to gpu
        let buffers_reallocated = self.ring_buffer.push_back(
            &self.queued_lines,
            device,
            &mut command_encoder,
            &mut staging,
        );
        self.queued_lines.clear();

        // create bind group
        if (buffers_reallocated || self.bind_group.is_none())
            && let (Some(config_buffer), Some(index_buffer), Some(data_buffer)) = (
                &self.config_buffer,
                &self.ring_buffer.index_buffer,
                &self.ring_buffer.data_buffer,
            )
        {
            tracing::debug!("creating waterfall bind group");

            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("waterfall"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: config_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: index_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: data_buffer.as_entire_binding(),
                    },
                ],
            }));
        }

        // commit staging transaction
        staging.commit(&mut command_encoder);
    }
}

fn begin_staging_transaction<'a>(
    staging_pool: &'a mut Option<StagingPool>,
    lines: &[WaterfallLine],
) -> Option<StagingTransaction> {
    // we need a non-empty line to get an estimate of how large they are.
    let line_length = lines.iter().find_map(|line| {
        if line.data.len() > 0 {
            Some(line.data.len())
        }
        else {
            None
        }
    })?;

    // get staging transaction
    let staging = staging_pool
        .get_or_insert_with(|| {
            let chunk_size = ChunkSize {
                chunk_size: u64::try_from((line_length * size_of::<f32>()).max(0x4000)).unwrap(),
                adaptive: true,
            };
            tracing::debug!(?chunk_size, "creating staging buffer");
            StagingPool::new(chunk_size, "waterfall-staging")
        })
        .begin();

    Some(staging)
}

#[derive(Debug, Default)]
struct RingBuffer {
    line_capacity: usize,

    index_buffer_allocator: RingBufferAllocator,
    index: VecDeque<IndexEntry>,
    index_buffer: Option<wgpu::Buffer>,

    data_buffer_allocator: RingBufferAllocator,
    data_buffer: Option<wgpu::Buffer>,
}

impl RingBuffer {
    fn set_line_capacity(&mut self, line_capacity: usize) {
        // note: we don't need to check if the index buffer is still large enough. this
        // will be done in `Self::push_back`

        self.line_capacity = line_capacity;
    }

    fn truncate_front(&mut self, new_length: usize) {
        let num_drop = self.index.len().saturating_sub(new_length);
        if num_drop > 0 {
            for _ in 0..num_drop {
                debug_assert!(self.pop_front().is_some());
            }
        }
    }

    fn pop_front(&mut self) -> Option<IndexEntry> {
        let line = self.index.pop_front()?;
        assert!(
            self.data_buffer_allocator
                .free_front(line.data_buffer_slice),
            "failed to free front:\nline = {line:?}\nallocator = {:#?}",
            self.data_buffer_allocator,
        );
        assert!(
            self.index_buffer_allocator
                .free_front(Slice::new(Range::from_start_and_length(
                    line.index_buffer_position,
                    1
                ))),
            "failed to free front:\nline = {line:?}\nallocator = {:#?}",
            self.index_buffer_allocator,
        );
        Some(line)
    }

    fn push_back(
        &mut self,
        lines: &[WaterfallLine],
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
        staging: &mut StagingTransaction,
    ) -> bool {
        if self.line_capacity == 0 || lines.is_empty() {
            return false;
        }

        // check if we can skip some queued lines
        let num_skip = lines.len().saturating_sub(self.line_capacity);
        let lines = &lines[num_skip..];

        // truncate from front to make enough room
        self.truncate_front(self.line_capacity - lines.len());

        let mut reallocated = false;
        let mut rebuild_index_buffer = false;

        // check if the index buffer has correct capacity. if not, remove the current
        // index buffer. we'll allocate it later.
        let required_index_buffer_size = u64::try_from(
            size_of::<IndexBufferHeader>() + size_of::<IndexBufferEntry>() * self.line_capacity,
        )
        .unwrap();
        if self
            .index_buffer
            .as_ref()
            .is_none_or(|index_buffer| index_buffer.size() < required_index_buffer_size)
        {
            // we'll also have to fix all entries on our side + the allocator
            tracing::debug!(
                ?self.line_capacity,
                ?required_index_buffer_size,
                index_buffer.size = ?self.index_buffer.as_ref().map(|index_buffer| index_buffer.size()),
                "will rebuild index buffer because index buffer is too small"
            );

            rebuild_index_buffer = true;
        }

        // check if the index buffer allocator has correct capacity. if we're already
        // rebuilding we need to reset it too.
        if rebuild_index_buffer
            || self.index_buffer_allocator.capacity() < u64::try_from(self.line_capacity).unwrap()
        {
            if !rebuild_index_buffer {
                tracing::debug!(
                    ?self.line_capacity,
                    index_buffer_allocator.capacity = ?self.index_buffer_allocator.capacity(),
                    "will rebuild index buffer because index buffer allocator is too small"
                );
            }

            self.index_buffer_allocator =
                RingBufferAllocator::new(u64::try_from(self.line_capacity).unwrap());

            rebuild_index_buffer = true;
        }

        let mut create_data_buffer = |capacity| {
            tracing::debug!(capacity, "creating waterfall data buffer");
            reallocated = true;
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("waterfall data buffer"),
                size: capacity,
                usage: wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };

        // check available space in data buffer
        let required_data_buffer_size = lines.iter().map(|line| line.bytes_len()).sum::<u64>()
            + self.data_buffer_allocator.len();

        // this is the estimated total buffer capacity, if the buffer was filled to line
        // capacity with a line size from the first line
        let estimated_required_data_buffer_size =
            lines.first().unwrap().bytes_len() * u64::try_from(self.line_capacity).unwrap();

        // get data buffer, allocate a new one if needed
        let data_buffer = self.data_buffer.get_or_insert_with(|| {
            let capacity = estimated_required_data_buffer_size.max(required_data_buffer_size);

            assert!(self.data_buffer_allocator.is_empty());
            self.data_buffer_allocator = RingBufferAllocator::new(capacity);

            create_data_buffer(capacity)
        });

        if required_data_buffer_size > self.data_buffer_allocator.capacity() {
            // need to reallocate
            tracing::debug!(
                required_data_buffer_size,
                capacity = self.data_buffer_allocator.capacity(),
                "need to reallocate data buffer"
            );

            assert!(self.index_buffer.is_some());

            let new_capacity = (self.data_buffer_allocator.capacity() * 2)
                .max(required_data_buffer_size)
                .max(estimated_required_data_buffer_size);
            let mut new_allocator = RingBufferAllocator::new(new_capacity);

            // allocate new buffer
            let new_buffer = create_data_buffer(new_capacity);

            // copy from old to new buffer
            for range in self.data_buffer_allocator.allocated().iter() {
                let [new_range, new_range_empty] = new_allocator
                    .allocate_back(range.len())
                    .expect("new allocator doesn't have enough space")
                    .parts();
                assert!(new_range_empty.is_empty(), "new allocation not contigious");
                assert_eq!(range.len(), new_range.len());

                command_encoder.copy_buffer_to_buffer(
                    data_buffer,
                    range.start,
                    &new_buffer,
                    new_range.start,
                    range.len(),
                );
            }

            // fix index
            // the new allocations should now be contiguous and in the same order as before
            let mut cursor = 0;
            assert_eq!(new_allocator.allocated().start(), 0);
            for line in &mut self.index {
                let start = cursor;
                let end = cursor + line.data_buffer_slice.len();
                cursor = end;
                line.data_buffer_slice = Slice::new(Range::new(start, end));
                //assert!(new_allocator.contains(line.data_buffer_slice));
            }
            assert_eq!(new_allocator.allocated().end(), cursor);

            // we'll fix the index buffer later
            tracing::debug!("will rebuild index buffer because data buffer was reallocated");
            rebuild_index_buffer = true;

            self.data_buffer_allocator = new_allocator;
            *data_buffer = new_buffer;
        }

        // if we need to rebuild the index buffer we will need to add all existing index
        // entries to the allocator now
        if rebuild_index_buffer {
            assert!(self.index_buffer_allocator.is_empty());

            for entry in &mut self.index {
                entry.index_buffer_position = self
                    .index_buffer_allocator
                    .allocate_back(1)
                    .expect("allocation failed")
                    .parts()[0]
                    .start;
            }
        }

        fn index_buffer_entry_slice(i: u64) -> std::ops::Range<u64> {
            let start = i * u64::try_from(size_of::<IndexBufferEntry>()).unwrap()
                + u64::try_from(size_of::<IndexBufferHeader>()).unwrap();
            let end = start + u64::try_from(size_of::<IndexBufferEntry>()).unwrap();

            start..end
        }

        // add new lines to index. this allocates space for them in the index and data
        // buffers
        for line in lines {
            let index_buffer_position = self
                .index_buffer_allocator
                .allocate_back(1)
                .expect("allocation failed")
                .parts()[0]
                .start;

            let data_buffer_slice = self
                .data_buffer_allocator
                .allocate_back(line.bytes_len())
                .expect("allocation failed");

            // add index entry
            let entry = self.index.push_back_mut(IndexEntry {
                index_buffer_position,
                data_buffer_slice,
                start_frequency: line.start_frequency,
                end_frequency: line.end_frequency,
            });

            // copy data to on-GPU ring buffer
            let data = bytemuck::cast_slice(&line.data);

            for (source, destination) in data_buffer_slice.iter_with_source() {
                // note: the destination offsets need to be aligned to
                // wgpu::COPY_BUFFER_ALIGNMENT (4 bytes).
                staging.write_buffer_from_slice(
                    data_buffer.slice(destination),
                    &data[source],
                    device,
                    command_encoder,
                );
            }

            if !rebuild_index_buffer {
                // we have an index buffer that we can immediately write the new entries to
                let index_buffer = self.index_buffer.as_ref().unwrap();

                staging.write_buffer_from_slice(
                    index_buffer.slice(index_buffer_entry_slice(index_buffer_position)),
                    bytemuck::bytes_of(&entry.buffer_entry()),
                    device,
                    command_encoder,
                );
            }
        }

        assert!(self.index.len() <= self.line_capacity);

        let index_header = {
            let allocated = self.index_buffer_allocator.allocated();

            IndexBufferHeader {
                capacity: self.index_buffer_allocator.capacity().try_into().unwrap(),
                start: allocated.start().try_into().unwrap(),
                end: allocated.end().try_into().unwrap(),
                length: allocated.len().try_into().unwrap(),
            }
        };

        if rebuild_index_buffer {
            // we need to ship the whole index buffer. instead of copying to a possibly
            // existing one through staging we might as well create a new one.

            let capacity = u64::try_from(size_of::<IndexBufferHeader>()).unwrap()
                + u64::try_from(size_of::<IndexBufferEntry>()).unwrap()
                    * self.index_buffer_allocator.capacity();
            tracing::debug!(capacity, "creating waterfall index buffer");

            let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("waterfall data buffer"),
                size: capacity,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: true,
            });

            {
                let mut view_mut = index_buffer.get_mapped_range_mut(..);

                view_mut
                    .slice(..size_of::<IndexBufferHeader>())
                    .copy_from_slice(bytemuck::bytes_of(&index_header));

                for line in &self.index {
                    let slice = index_buffer_entry_slice(line.index_buffer_position);
                    let slice = std::ops::Range {
                        start: usize::try_from(slice.start).unwrap(),
                        end: usize::try_from(slice.end).unwrap(),
                    };

                    view_mut
                        .slice(slice)
                        .copy_from_slice(bytemuck::bytes_of(&line.buffer_entry()));
                }
            }

            index_buffer.unmap();

            self.index_buffer = Some(index_buffer);
            reallocated = true;
        }
        else {
            // we still need to update the header

            let index_buffer = self.index_buffer.as_ref().unwrap();
            staging.write_buffer_from_slice(
                index_buffer.slice(..u64::try_from(size_of::<IndexBufferHeader>()).unwrap()),
                bytemuck::bytes_of(&index_header),
                device,
                command_encoder,
            );
        }

        reallocated
    }
}

#[derive(Clone, Debug)]
pub struct WaterfallLine {
    pub data: Vec<f32>,
    pub start_frequency: f32,
    pub end_frequency: f32,
}

impl WaterfallLine {
    fn bytes_len(&self) -> u64 {
        u64::try_from(size_of::<f32>() * self.data.len()).unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
struct IndexEntry {
    index_buffer_position: u64,
    data_buffer_slice: Slice,
    start_frequency: f32,
    end_frequency: f32,
}

impl IndexEntry {
    fn buffer_entry(&self) -> IndexBufferEntry {
        IndexBufferEntry {
            start_offset: u32::try_from(
                self.data_buffer_slice.start() / u64::try_from(size_of::<f32>()).unwrap(),
            )
            .unwrap(),
            end_offset: u32::try_from(
                self.data_buffer_slice.end() / u64::try_from(size_of::<f32>()).unwrap(),
            )
            .unwrap(),
            start_frequency: self.start_frequency,
            end_frequency: self.end_frequency,
        }
    }
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
struct IndexBufferHeader {
    capacity: u32,
    start: u32,
    end: u32,
    length: u32,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
struct IndexBufferEntry {
    start_offset: u32,
    end_offset: u32,
    start_frequency: f32,
    end_frequency: f32,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable, Default, PartialEq)]
#[repr(C)]
struct ConfigData {
    min_db: f32,
    max_db: f32,
    start_frequency: f32,
    end_frequency: f32,
    background_color: [f32; 4],
    foreground_color1: [f32; 4],
    foreground_color2: [f32; 4],
}

impl ConfigData {
    pub fn new(
        style: &WaterfallStyle,
        min_db: f32,
        max_db: f32,
        start_frequency: f32,
        end_frequency: f32,
    ) -> Self {
        Self {
            min_db,
            max_db,
            start_frequency,
            end_frequency,
            background_color: color32_to_linrgba(style.background_color),
            foreground_color1: color32_to_linrgba(style.foreground_color1),
            foreground_color2: color32_to_linrgba(style.foreground_color2),
        }
    }
}
