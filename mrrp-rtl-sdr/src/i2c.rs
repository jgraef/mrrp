#[derive(Debug)]
pub struct I2cRepeater<'a> {
    usb_interface: &'a mut nusb::Interface,
}

impl<'a> I2cRepeater<'a> {
    pub fn new(usb_interface: &'a mut nusb::Interface) -> Self {
        Self { usb_interface }
    }
}
