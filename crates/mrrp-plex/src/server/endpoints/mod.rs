pub mod events;
pub mod streams;

pub trait AsCloseCode {
    fn as_close_code(&self) -> u16;
}
