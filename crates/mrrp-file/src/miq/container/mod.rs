//! # mrrp IQ container file formrat
//!
//! The mrrp IQ file format (*MIQ* for short) is a simple file format for
//! storing and transmission of IQ data. It is inspired by [RIFF][1] and other
//! chunked formats. This module only handles chunking and serialization for
//! structured chunks.
//!
//! ## Basic structure
//!
//! On the lowest level a MIQ file contains a sequence of chunks, starting from
//! the start of the file. A chunk starts with a header:
//!
//! +--------+------+------+--------------------------------------------------------+
//! | Offset | Size | Name    | Description |                                       |
//! +--------+------+------+--------------------------------------------------------+
//! | 0x00   | 0x04 | `tag`   | Specifies how the chunk is to be interpreted.       |
//! | 0x04   | 0x04 | `flags` | Flags that modify how a chunk is to be interpreted. |
//! | 0x08   | 0x08 | `size`  | The size of the chunk contents in bytes.            |
//! +--------+------+------+--------------------------------------------------------+
//!
//! `flags` and `size` are encoded as big endian.
//!
//! The contents of a chunk follow immediately after the header. The size of the
//! contents is defined by `size`. The contents are padded to a length of a
//! multiple of 8 bytes, but at least to a length of 8 bytes.
//!
//! ### Tags
//!
//! Tags define the chunk type and how the contents of the chunk are to be
//! interpreted. Tags consist of 4 bytes that must be printable ASCII and must
//! not contain `\0` bytes. Chunks with unknown tags should be ignored. The
//! `size` field specifies the size of the *contents* of the chunk; this it
//! doesn't include the 16 bytes of the chunk header.
//!
//! ### Flags
//!
//! +-------+-------------+
//! | Value | Description |
//! +-------+-------------+
//! | 0x00000001 | The chunk contains nested chunks. This means the contents of this chunk can be read with a nested MIQ chunk reader |
//! | 0x00000002 | The chunk should be ignored. This can be used to delete chunks by just setting a single bit. The chunk will still be physically there, but should be ignored by readers. |
//! +-------+-------------+
//!
//! Flags modify how a chunk is to be interpreted. Unknown flags should be
//! ignored. The flag field is a 32-bit integer, encoded as big endian.
//!
//! ### Structured data
//!
//! Oftentimes one wishes to encode somewhat arbitrary or extensible data into
//! the contents of a chunk. Well-known formats like JSON and XML serve this
//! purpose, but use more space, since they're human-readable. MIQ uses
//! [CBOR][2] to encode structured data into its chunks. It is similar to JSON,
//! but encodes the data in a binary format. Note that like JSON or XML, CBOR is
//! self-describing. This means the encoded data can be decoded without knowing
//! anything about its contents. This is useful when a decoder only knows about
//! a few fields in the encoded data, because it can just ignore the rest.
//!
//! ## MIQ header chunk
//!
//! Every MIQ file must start with a `b"MIQ0"` chunk. This chunk serves as a
//! file header for easy recognition by other tools. The chunk contents are
//! CBOR-encoded.
//!
//! TODO: Specify header struct.
//!
//! [1]: https://en.wikipedia.org/wiki/Resource_Interchange_File_Format
//! [2]: https://cbor.io/

use crate::miq::container::header::Version;

pub mod chunk;
pub mod header;
pub mod reader;
pub mod writer;

pub const VERSION: Version = Version {
    major: 0,
    minor: 1,
    patch: 0,
};

#[cfg(test)]
mod tests {
    use std::io::Read;

    use crate::miq::container::{
        chunk::{
            Flags,
            Tag,
        },
        reader::Reader,
        writer::Writer,
    };

    #[test]
    fn write_read_single_chunk() {
        let mut buf: Vec<u8> = vec![];

        let mut writer = Writer::new(&mut buf);
        writer
            .write_data_chunk(
                Tag::from_bytes(*b"TEST").unwrap(),
                Default::default(),
                b"Hello World!",
            )
            .unwrap();

        let mut reader = Reader::new(&buf[..]);
        let mut read_chunk = reader.read_chunk().unwrap();

        let chunk_header = read_chunk.chunk_header();
        assert_eq!(chunk_header.tag, Tag::from_bytes(*b"TEST").unwrap());
        assert_eq!(chunk_header.flags, Flags::default());
        assert_eq!(chunk_header.size, 12);

        let mut chunk_data = vec![];
        read_chunk.read_to_end(&mut chunk_data).unwrap();
        assert_eq!(chunk_data, b"Hello World!")
    }

    #[test]
    fn write_read_two_chunks() {
        let mut buf: Vec<u8> = vec![];

        let mut writer = Writer::new(&mut buf);
        writer
            .write_data_chunk(
                Tag::from_bytes(*b"TES1").unwrap(),
                Default::default(),
                b"Hello",
            )
            .unwrap();
        writer
            .write_data_chunk(
                Tag::from_bytes(*b"TES2").unwrap(),
                Default::default(),
                b" World!",
            )
            .unwrap();

        let mut reader = Reader::new(&buf[..]);

        let mut read_chunk = reader.read_chunk().unwrap();

        let chunk_header = read_chunk.chunk_header();
        assert_eq!(chunk_header.tag, Tag::from_bytes(*b"TES1").unwrap());
        assert_eq!(chunk_header.flags, Flags::default());
        assert_eq!(chunk_header.size, 5);

        let mut chunk_data = vec![];
        read_chunk.read_to_end(&mut chunk_data).unwrap();
        assert_eq!(chunk_data, b"Hello");

        let mut read_chunk = reader.read_chunk().unwrap();

        let chunk_header = read_chunk.chunk_header();
        assert_eq!(chunk_header.tag, Tag::from_bytes(*b"TES2").unwrap());
        assert_eq!(chunk_header.flags, Flags::default());
        assert_eq!(chunk_header.size, 7);

        let mut chunk_data = vec![];
        read_chunk.read_to_end(&mut chunk_data).unwrap();
        assert_eq!(chunk_data, b" World!");
    }

    #[test]
    fn write_read_second_chunk() {
        let mut buf: Vec<u8> = vec![];

        let mut writer = Writer::new(&mut buf);
        writer
            .write_data_chunk(
                Tag::from_bytes(*b"TES1").unwrap(),
                Default::default(),
                b"Hello",
            )
            .unwrap();
        writer
            .write_data_chunk(
                Tag::from_bytes(*b"TES2").unwrap(),
                Default::default(),
                b" World!",
            )
            .unwrap();

        let mut reader = Reader::new(&buf[..]);

        let read_chunk = reader.read_chunk().unwrap();

        let chunk_header = read_chunk.chunk_header();
        assert_eq!(chunk_header.tag, Tag::from_bytes(*b"TES1").unwrap());
        assert_eq!(chunk_header.flags, Flags::default());
        assert_eq!(chunk_header.size, 5);

        let mut read_chunk = reader.read_chunk().unwrap();

        let chunk_header = read_chunk.chunk_header();
        assert_eq!(chunk_header.tag, Tag::from_bytes(*b"TES2").unwrap());
        assert_eq!(chunk_header.flags, Flags::default());
        assert_eq!(chunk_header.size, 7);

        let mut chunk_data = vec![];
        read_chunk.read_to_end(&mut chunk_data).unwrap();
        assert_eq!(chunk_data, b" World!");
    }
}
