use std::fmt::Debug;

pub struct SkipDebug;

impl Debug for SkipDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "...")
    }
}

pub fn skip_debug<T>(x: &T) -> SkipDebug {
    let _ = x;
    SkipDebug
}

pub struct BufferDebug {
    length: usize,
}

impl Debug for BufferDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.length == 0 {
            write!(f, "[]")
        }
        else {
            write!(f, "[size={}]", self.length)
        }
    }
}

pub fn buffer_debug<'a, T>(buffer: &'a [T]) -> BufferDebug {
    BufferDebug {
        length: buffer.len(),
    }
}
