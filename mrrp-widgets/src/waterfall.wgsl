
struct Config {
    min_db: f32,
    max_db: f32,
    start_frequency: f32,
    end_frequency: f32,
    background_color: vec4f,
    foreground_color1: vec4f,
    foreground_color2: vec4f,
}

struct Index {
    capacity: u32,
    start: u32,
    end: u32,
    length: u32,
    entries: array<IndexEntry>,
}

struct IndexEntry {
    start_offset: u32,
    end_offset: u32,
    start_frequency: f32,
    end_frequency: f32,
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
var<uniform> waterfall_config: Config;

@group(0)
@binding(1)
var<storage, read> waterfall_index: Index;

@group(0)
@binding(2)
var<storage, read> waterfall_data: array<f32>;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    let uv = vec2f(
        f32((input.vertex_index & 1) << 1),
        f32((input.vertex_index & 2))
    );

    // draw screen-filling tri
    output.fragment_position = vec4f(
        uv * 2.0 - 1.0,
        1.0, // that's what egui_wgpu clears the depth buffer to
        1.0,
    );

    output.uv = vec2f(uv.x, 1.0 - uv.y);

    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;
    output.color = waterfall_config.background_color;

    // get index entry for line
    let line_index = u32(input.uv.y * f32(waterfall_index.capacity - 1));

    if line_index < waterfall_index.length {
        // caculate entry index. the extra +capacity term is to avoid wrapping around into negative numbers.
        let entry_index = (waterfall_index.end + waterfall_index.capacity - line_index - 1) % waterfall_index.capacity;

        let entry = waterfall_index.entries[entry_index];

        // the frequency displayed at this pixel
        let f = mix(waterfall_config.start_frequency, waterfall_config.end_frequency, input.uv.x);

        // calculate where inside or outside of the data for the line we fall.
        let k = (f - entry.start_frequency) / (entry.end_frequency - entry.start_frequency);
        if k > 0.0 && k < 1.0 {
            // data index
            let data_index = (u32(k * f32(entry.end_offset - entry.start_offset - 1)) + entry.start_offset) % arrayLength(&waterfall_data);

            // get the value to be displayed
            let value_linear = waterfall_data[data_index];
            let value_db = linear_to_db(value_linear);
            let value_scaled = (value_db - waterfall_config.min_db) / (waterfall_config.max_db - waterfall_config.min_db);
            let value_clamped = clamp(value_scaled, 0.0, 1.0);
            //let value_clamped = clamp(value_linear, 0.0, 1.0);

            // for now the color will just be a linear mix
            output.color = mix(waterfall_config.foreground_color1, waterfall_config.foreground_color2, value_clamped);
            //output.color = mix(waterfall_config.foreground_color1, waterfall_config.foreground_color2, k);

            //let test = f32(data_index) / f32(arrayLength(&waterfall_data));
            //let test = f32(entry_index) / f32(waterfall_index.capacity);
            //output.color = mix(waterfall_config.foreground_color1, waterfall_config.foreground_color2, test);

            //output.color = vec4f(1.0, 0.0, 0.0, 1.0);
        }
    }

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
