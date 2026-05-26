use egui::{
    Color32,
    Pos2,
    Sense,
    Vec2,
    epaint::{
        ColorMode,
        PathShape,
        PathStroke,
    },
};
use enterpolation::{
    Curve,
    Signal,
    linear::LinearBuilder,
    utils::lerp,
};
use num_traits::real::Real;

#[derive(Debug)]
pub struct SpectrumView<'a> {
    data: &'a SpectrumData,
    desired_size: Vec2,
    style: SpectrumStyle,
    y_scale: f32,
    y_offset: f32,
}

impl<'a> SpectrumView<'a> {
    pub fn new(data: &'a SpectrumData) -> Self {
        Self {
            data,
            desired_size: Vec2::INFINITY,
            style: Default::default(),
            y_scale: 1.0,
            y_offset: 0.0,
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
}

impl<'a> egui::Widget for SpectrumView<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = self.desired_size.min(ui.available_size());
        let (response, painter) = ui.allocate_painter(desired_size, Sense::CLICK);

        let amplitude = LinearBuilder::new()
            .elements(&self.data.data)
            .equidistant()
            .domain(response.rect.min.x, response.rect.max.x)
            .build()
            .unwrap();

        //let num_samples = response.rect.width().ceil() as usize;
        let num_samples = 10;

        // interpolated points on the curve
        let points = Points::new(amplitude).take(num_samples).map(|(x, y)| {
            //let y = (y * self.y_scale + self.y_offset).clamp(0.0, 1.0);
            let y = lerp(response.rect.max.y, response.rect.min.y, y);

            Pos2::new(x, y)
        });

        // add a point before and after the curve to finish the shape
        let points = std::iter::once(Pos2::new(response.rect.min.x, response.rect.max.y))
            .chain(points)
            .chain(std::iter::once(Pos2::new(
                response.rect.max.x,
                response.rect.max.y,
            )));

        // collect to vec
        let points = points.collect::<Vec<_>>();

        painter.add(PathShape {
            points,
            closed: true,
            fill: self.style.fill,
            stroke: self
                .style
                .line
                .as_ref()
                .map_or_else(Default::default, |line| {
                    PathStroke {
                        width: line.width,
                        color: ColorMode::Solid(line.color),
                        kind: egui::StrokeKind::Middle,
                    }
                }),
        });

        response
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumData {
    pub data: Vec<f32>,
    pub start_frequency: f32,
    pub end_frequency: f32,
}

#[derive(Clone, Debug)]
pub struct SpectrumStyle {
    pub fill: Color32,
    pub line: Option<SpectrumLine>,
}

impl Default for SpectrumStyle {
    fn default() -> Self {
        Self {
            fill: Color32::from_white_alpha(128),
            line: Some(Default::default()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumLine {
    pub width: f32,
    pub color: Color32,
}

impl Default for SpectrumLine {
    fn default() -> Self {
        Self {
            width: 4.0,
            color: Color32::PURPLE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Points<C> {
    inner: C,
}

impl<C> Points<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<R, C> Signal<R> for Points<C>
where
    R: Clone,
    C: Signal<R>,
{
    type Output = (R, <C as Signal<R>>::Output);

    fn eval(&self, input: R) -> Self::Output {
        (input.clone(), self.inner.eval(input))
    }
}

impl<R, C> Curve<R> for Points<C>
where
    R: Real,
    C: Curve<R>,
{
    fn domain(&self) -> [R; 2] {
        self.inner.domain()
    }
}
