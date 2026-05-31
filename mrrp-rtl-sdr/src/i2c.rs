#[derive(Debug)]
pub struct I2cRepeater<'a> {
    _usb_interface: &'a mut nusb::Interface,
}

impl<'a> I2cRepeater<'a> {
    pub fn new(_usb_interface: &'a mut nusb::Interface) -> Self {
        Self { _usb_interface }
    }
}
