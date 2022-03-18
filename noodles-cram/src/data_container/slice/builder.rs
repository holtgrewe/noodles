use std::{
    cmp,
    collections::{HashMap, HashSet},
    io,
};

use md5::{Digest, Md5};
use noodles_core::Position;
use noodles_fasta as fasta;
use noodles_sam::{self as sam, AlignmentRecord};

use crate::{
    container::{
        block::{self, CompressionMethod},
        Block, ReferenceSequenceId,
    },
    data_container::{
        compression_header::{data_series_encoding_map::DataSeries, SubstitutionMatrix},
        CompressionHeader,
    },
    record::{Feature, Features, Flags},
    writer, BitWriter, Record,
};

use super::{Header, Slice};

use noodles_bam as bam;

const CORE_DATA_BLOCK_CONTENT_ID: i32 = 0;
const MAX_RECORD_COUNT: usize = 10240;

#[derive(Debug, Default)]
pub struct Builder {
    records: Vec<Record>,
    slice_reference_sequence_id: Option<bam::record::ReferenceSequenceId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddRecordError {
    ReferenceSequenceIdMismatch(Record),
    SliceFull(Record),
}

impl Builder {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn add_record(&mut self, record: Record) -> Result<&Record, AddRecordError> {
        if self.records.len() >= MAX_RECORD_COUNT {
            return Err(AddRecordError::SliceFull(record));
        }

        if self.records.is_empty() {
            self.slice_reference_sequence_id = record.reference_sequence_id();
        }

        if record.reference_sequence_id() == self.slice_reference_sequence_id {
            self.records.push(record);
            Ok(self.records.last().unwrap())
        } else {
            Err(AddRecordError::ReferenceSequenceIdMismatch(record))
        }
    }

    pub fn build(
        mut self,
        reference_sequence_repostitory: &fasta::repository::Repository,
        header: &sam::Header,
        compression_header: &CompressionHeader,
        record_counter: i64,
    ) -> io::Result<Slice> {
        let slice_reference_sequence_id = find_slice_reference_sequence_id(&self.records);

        let (slice_alignment_start, slice_alignment_end) = if slice_reference_sequence_id.is_some()
        {
            find_slice_alignment_positions(&self.records)?
        } else {
            (None, None)
        };

        let (core_data_block, external_blocks) = write_records(
            compression_header,
            slice_reference_sequence_id,
            slice_alignment_start,
            &mut self.records,
        )?;

        let mut block_content_ids = Vec::with_capacity(external_blocks.len() + 1);
        block_content_ids.push(core_data_block.content_id());

        for block in &external_blocks {
            block_content_ids.push(block.content_id());
        }

        let reference_md5 = match (
            slice_reference_sequence_id,
            slice_alignment_start,
            slice_alignment_end,
        ) {
            (ReferenceSequenceId::Some(id), Some(start), Some(end)) => {
                let reference_sequence_name = header
                    .reference_sequences()
                    .get_index(id as usize)
                    .map(|(_, rs)| rs.name())
                    .expect("invalid reference sequence ID");

                let reference_sequence = reference_sequence_repostitory
                    .get(reference_sequence_name)
                    .expect("missing reference sequence")
                    .expect("invalid reference sequence");

                let mut hasher = Md5::new();
                hasher.update(&reference_sequence[start..=end]);
                <[u8; 16]>::from(hasher.finalize())
            }
            _ => [0; 16],
        };

        let mut builder = Header::builder()
            .set_reference_sequence_id(slice_reference_sequence_id)
            .set_record_count(self.records.len())
            .set_record_counter(record_counter)
            .set_block_count(block_content_ids.len())
            .set_block_content_ids(block_content_ids)
            .set_reference_md5(reference_md5);

        if let (Some(alignment_start), Some(alignment_end)) =
            (slice_alignment_start, slice_alignment_end)
        {
            let alignment_span = usize::from(alignment_end) - usize::from(alignment_start) + 1;

            builder = builder
                .set_alignment_start(alignment_start)
                .set_alignment_span(alignment_span);
        }

        let header = builder.build();

        Ok(Slice::new(header, core_data_block, external_blocks))
    }
}

fn find_slice_reference_sequence_id(records: &[Record]) -> ReferenceSequenceId {
    assert!(!records.is_empty());

    let reference_sequence_ids: HashSet<_> = records
        .iter()
        .map(|record| record.reference_sequence_id())
        .collect();

    match reference_sequence_ids.len() {
        0 => unreachable!(),
        1 => reference_sequence_ids
            .into_iter()
            .next()
            .map(|reference_sequence_id| match reference_sequence_id {
                Some(id) => ReferenceSequenceId::Some(usize::from(id) as i32),
                None => ReferenceSequenceId::None,
            })
            .expect("reference sequence IDs cannot be empty"),
        _ => ReferenceSequenceId::Many,
    }
}

fn find_slice_alignment_positions(
    records: &[Record],
) -> io::Result<(Option<Position>, Option<Position>)> {
    assert!(!records.is_empty());

    let mut slice_alignment_start = Position::new(usize::MAX);
    let mut slice_alignment_end = None;

    for record in records {
        slice_alignment_start = cmp::min(record.alignment_start(), slice_alignment_start);
        slice_alignment_end = cmp::max(record.alignment_end(), slice_alignment_end);
    }

    Ok((slice_alignment_start, slice_alignment_end))
}

fn write_records(
    compression_header: &CompressionHeader,
    slice_reference_sequence_id: ReferenceSequenceId,
    slice_alignment_start: Option<Position>,
    records: &mut [Record],
) -> io::Result<(Block, Vec<Block>)> {
    let mut core_data_writer = BitWriter::new(Vec::new());

    let mut external_data_writers = HashMap::new();

    for i in 0..DataSeries::LEN {
        let block_content_id = (i + 1) as i32;
        external_data_writers.insert(block_content_id, Vec::new());
    }

    for &block_content_id in compression_header.tag_encoding_map().keys() {
        external_data_writers.insert(block_content_id, Vec::new());
    }

    let mut record_writer = writer::record::Writer::new(
        compression_header,
        &mut core_data_writer,
        &mut external_data_writers,
        slice_reference_sequence_id,
        slice_alignment_start,
    );

    for record in records {
        update_substitution_features(
            compression_header.preservation_map().substitution_matrix(),
            &mut record.features,
        );

        // FIXME: For simplicity, all records are written as detached.
        record.cram_bit_flags.insert(Flags::DETACHED);
        record.cram_bit_flags.remove(Flags::HAS_MATE_DOWNSTREAM);
        record.distance_to_next_fragment = None;

        record_writer.write_record(record)?;
    }

    let core_data_block = core_data_writer.finish().and_then(|buf| {
        Block::builder()
            .set_content_type(block::ContentType::CoreData)
            .set_content_id(CORE_DATA_BLOCK_CONTENT_ID)
            .compress_and_set_data(buf, CompressionMethod::Gzip)
            .map(|builder| builder.build())
    })?;

    let external_blocks: Vec<_> = external_data_writers
        .into_iter()
        .filter(|(_, buf)| !buf.is_empty())
        .map(|(block_content_id, buf)| {
            Block::builder()
                .set_content_type(block::ContentType::ExternalData)
                .set_content_id(block_content_id)
                .compress_and_set_data(buf, CompressionMethod::Gzip)
                .map(|builder| builder.build())
        })
        .collect::<Result<_, _>>()?;

    Ok((core_data_block, external_blocks))
}

fn update_substitution_features(substitution_matrix: &SubstitutionMatrix, features: &mut Features) {
    use crate::record::feature::substitution;

    for feature in features.iter_mut() {
        match feature {
            Feature::Substitution(pos, substitution::Value::Bases(reference_base, read_base)) => {
                let code = substitution_matrix.find_code(*reference_base, *read_base);
                let value = substitution::Value::Code(code);
                *feature = Feature::Substitution(*pos, value);
            }
            Feature::Substitution(_, substitution::Value::Code(_)) => {
                panic!("cannot update substitution features with code");
            }
            _ => {}
        }
    }
}
