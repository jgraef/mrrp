use image::{
    Rgb,
    RgbImage,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum Channel {
    #[default]
    Green,
    Blue,
    Red,
}

impl Channel {
    pub fn next(self) -> Option<Self> {
        match self {
            Channel::Green => Some(Self::Blue),
            Channel::Blue => Some(Self::Red),
            Channel::Red => None,
        }
    }
}

pub trait FrameBuffer {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8;
}

impl<F> FrameBuffer for &F
where
    F: FrameBuffer,
{
    #[inline]
    fn width(&self) -> usize {
        (&**self).width()
    }

    #[inline]
    fn height(&self) -> usize {
        (&**self).height()
    }

    #[inline]
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8 {
        (&**self).channel(x, y, channel)
    }
}

impl FrameBuffer for RgbImage {
    #[inline]
    fn width(&self) -> usize {
        RgbImage::width(self).try_into().unwrap()
    }

    #[inline]
    fn height(&self) -> usize {
        RgbImage::height(self).try_into().unwrap()
    }

    #[inline]
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8 {
        let pixel = self.get_pixel(x.try_into().unwrap(), y.try_into().unwrap());
        match channel {
            Channel::Green => pixel.0[1],
            Channel::Blue => pixel.0[2],
            Channel::Red => pixel.0[0],
        }
    }
}

pub trait FrameBufferMut {
    fn set_size(&mut self, width: usize, height: usize);
    fn set_channel(&mut self, x: usize, y: usize, channel: Channel, value: u8);
}

impl<F> FrameBufferMut for &mut F
where
    F: FrameBufferMut,
{
    fn set_size(&mut self, width: usize, height: usize) {
        (&mut **self).set_size(width, height);
    }

    fn set_channel(&mut self, x: usize, y: usize, channel: Channel, value: u8) {
        (&mut **self).set_channel(x, y, channel, value);
    }
}

impl FrameBufferMut for RgbImage {
    fn set_size(&mut self, width: usize, height: usize) {
        //*self = RgbImage::new(width.try_into().unwrap(), height.try_into().unwrap());
        *self = RgbImage::from_fn(
            width.try_into().unwrap(),
            height.try_into().unwrap(),
            |_x, _y| Rgb([0xff, 0, 0xff]),
        );
    }

    fn set_channel(&mut self, x: usize, y: usize, channel: Channel, value: u8) {
        let pixel = self.get_pixel_mut(x.try_into().unwrap(), y.try_into().unwrap());
        match channel {
            Channel::Green => pixel.0[1] = value,
            Channel::Blue => pixel.0[2] = value,
            Channel::Red => pixel.0[0] = value,
        };
    }
}
