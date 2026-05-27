
struct WaterfallConfig {
    min_db: f32,
    max_db: f32,
    //_padding: [u32; 2],
    background_color: vec4f,
    foreground_color1: vec4f,
    foreground_color2: vec4f,
}


struct WaterfallIndex {
    lines: array<WaterfallIndexLine>
}

struct WaterfallIndexLine {
    data_offset: u32,
    data_len: u32,
    frequency_start: f32,
    frequency_end: f32,
}

struct WaterfallData {
    data: array<f32>,
}

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOutput {
    @builtin(position) fragment_position: vec4f,
    @location(0) uv: vec2f,
}

struct FragmentOutput {
    @location(0) color: vec4f,
}

@group(0)
@binding(0)
var<uniform> waterfall_config: WaterfallConfig;

@group(0)
@binding(1)
var<storage, read> waterfall_index: WaterfallIndex;

@group(0)
@binding(2)
var<storage, read> waterfall_data: WaterfallData;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.uv = vec2f(
        f32((input.vertex_index & 1) << 1),
        f32((input.vertex_index & 2))
    );

    // draw screen-filling tri
    output.fragment_position = vec4f(
        output.uv * 2.0 - 1.0,
        1.0, // that's what egui_wgpu clears the depth buffer to
        1.0,
    );

    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;

    output.color = vec4f(0.0, 0.0, 0.5, 1.0);

    return output;
}

// converts linear scale to dB
fn linear_to_db(value: f32) -> f32 {
    return 10.0 * log10(value);
}

fn log10(value: f32) -> f32 {
    // uses change-of-base identity to implement log10
    // note: we think log2 is faster than ln, because IEEE-754 floats can do this pretty well.

    // log_2(10)
    const LOG2_10: f32 = log2(10.0);

    return log2(value) / LOG2_10;
}
