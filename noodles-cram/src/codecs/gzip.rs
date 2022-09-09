use std::io::{self, Read, Write};

use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};

pub fn decode(src: &[u8], dst: &mut [u8]) -> io::Result<()> {
    let mut decoder = GzDecoder::new(src);
    decoder.read_exact(dst)
}

pub fn encode(src: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(src)?;
    encoder.finish()
}
