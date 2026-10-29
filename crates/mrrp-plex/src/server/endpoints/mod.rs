pub mod events;
pub mod stream;

pub trait AsCloseCode {
    fn as_close_code(&self) -> u16;
}
