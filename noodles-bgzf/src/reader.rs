use std::io::{self, Read, Seek, SeekFrom};

use byteorder::{ByteOrder, LittleEndian};
use flate2::read::DeflateDecoder;

use super::{gz, Block, BGZF_HEADER_SIZE};

pub struct Reader<R: Read> {
    inner: R,
    position: u64,
    cdata: Vec<u8>,
}

impl<R: Read> Reader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            position: 0,
            cdata: Vec::new(),
        }
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn read_block(&mut self, block: &mut Block) -> io::Result<usize> {
        // gzip header
        let mut header = [0; BGZF_HEADER_SIZE];

        if self.inner.read_exact(&mut header).is_err() {
            return Ok(0);
        }

        let bsize = &header[16..18];
        let block_size = LittleEndian::read_u16(bsize) as usize;

        // Add 1 because BSIZE is "total Block SIZE minus 1".
        let cdata_len = block_size - BGZF_HEADER_SIZE - gz::TRAILER_SIZE + 1;

        self.cdata.resize(cdata_len, Default::default());
        self.inner.read_exact(&mut self.cdata)?;

        let mut trailer = [0; gz::TRAILER_SIZE];
        self.inner.read_exact(&mut trailer)?;

        let mut decoder = DeflateDecoder::new(&self.cdata[..]);

        let block_buf = block.get_mut();
        block_buf.clear();

        decoder.read_to_end(block_buf)?;

        block.set_c_offset(self.position);
        block.set_position(0);

        self.position += BGZF_HEADER_SIZE as u64 + cdata_len as u64 + gz::TRAILER_SIZE as u64;

        Ok(block_size)
    }
}

impl<R: Read + Seek> Reader<R> {
    pub fn seek(&mut self, pos: u64, block: &mut Block) -> io::Result<u64> {
        let c_offset = compressed_offset(pos);
        let u_offset = uncompressed_offset(pos);

        self.inner.seek(SeekFrom::Start(c_offset))?;
        self.position = c_offset;

        self.read_block(block)?;
        block.seek(SeekFrom::Start(u_offset))?;

        Ok(pos)
    }
}

fn compressed_offset(offset: u64) -> u64 {
    // is the mask necessary?
    (offset >> 16) & 0xffff_ffff_ffff
}

fn uncompressed_offset(offset: u64) -> u64 {
    offset & 0xffff
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn test_read_block() -> io::Result<()> {
        let file = File::open("tests/fixtures/sample.gz")?;
        let mut reader = Reader::new(file);

        let mut block = Block::default();

        reader.read_block(&mut block)?;
        assert_eq!(&block.get_ref()[..], &b"noodles"[..]);

        reader.read_block(&mut block)?;
        assert_eq!(&block.get_ref()[..], &b"bgzf"[..]);

        reader.read_block(&mut block)?;
        assert!(block.get_ref().is_empty());

        assert_eq!(reader.read_block(&mut block)?, 0);

        Ok(())
    }

    #[test]
    fn test_compressed_offset() {
        assert_eq!(compressed_offset(88384945211), 1348647);
        assert_eq!(compressed_offset(188049630896), 2869409);
        assert_eq!(compressed_offset(26155658182977), 399103671);
    }

    #[test]
    fn test_uncompressed_offset() {
        assert_eq!(uncompressed_offset(88384945211), 15419);
        assert_eq!(uncompressed_offset(188049630896), 42672);
        assert_eq!(uncompressed_offset(26155658182977), 321);
    }
}
