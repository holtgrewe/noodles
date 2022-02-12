#![allow(dead_code)]

use std::io::{self, Read};

use super::RangeCoder;

#[derive(Clone, Debug)]
pub struct Model {
    total_freq: u32,
    symbols: Vec<u8>,
    frequencies: Vec<u32>,
}

impl Model {
    pub fn new(num_sym: u8) -> Self {
        let len = usize::from(num_sym);
        let mut symbols = Vec::with_capacity(len);
        let mut frequencies = Vec::with_capacity(len);

        for i in 0..num_sym {
            symbols.push(i);
            frequencies.push(u32::from(i));
        }

        Self {
            total_freq: u32::from(num_sym),
            symbols,
            frequencies,
        }
    }

    pub fn decode<R>(&mut self, reader: &mut R, range_coder: &mut RangeCoder) -> io::Result<u8>
    where
        R: Read,
    {
        let freq = range_coder.range_get_freq(self.total_freq);

        let mut acc = 0;
        let mut x = 0;

        while acc + self.frequencies[x] <= freq {
            acc += self.frequencies[x];
            x += 1;
        }

        range_coder.range_decode(reader, acc, self.frequencies[x])?;

        self.frequencies[x] += 16;
        self.total_freq += 16;

        if self.total_freq > (1 << 16) - 17 {
            self.renormalize();
        }

        let sym = self.symbols[x];

        if x > 0 && self.frequencies[x] > self.frequencies[x - 1] {
            self.frequencies.swap(x, x - 1);
            self.symbols.swap(x, x - 1);
        }

        Ok(sym)
    }

    fn renormalize(&mut self) {
        let mut total_freq = 0;

        for freq in &mut self.frequencies {
            *freq -= *freq / 2;
            total_freq += *freq;
        }

        self.total_freq = total_freq;
    }
}