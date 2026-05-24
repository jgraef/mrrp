
struct SpectrumConfig {
    min_db: f32,
    max_db: f32,
    f_resolution: u32,
    //_padding: u32,
    fg_color: vec4f,
    bg_color: vec4f,
}

struct SpectrumData {
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
var<storage, read> spectrum_config: SpectrumConfig;

@group(0)
@binding(1)
var<storage, read> spectrum_data: SpectrumData;

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

    let index = u32(input.uv.x * f32(spectrum_config.f_resolution - 1));
    let value = spectrum_data.data[index] - spectrum_config.min_db / (spectrum_config.max_db - spectrum_config.min_db);

    // 0 = fg, 1 = bg
    let fg_or_bg = step(value, input.uv.y);

    output.color = mix(spectrum_config.fg_color, spectrum_config.bg_color, fg_or_bg);

    return output;
}
