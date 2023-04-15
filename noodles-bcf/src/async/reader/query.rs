use std::vec;

use futures::{stream, Stream};
use noodles_bgzf as bgzf;
use noodles_core::{region::Interval, Position};
use noodles_csi::index::reference_sequence::bin::Chunk;
use tokio::io::{self, AsyncRead, AsyncSeek};

use super::Reader;
use crate::lazy::{self, record::ChromosomeId};

enum State {
    Seek,
    Read(bgzf::VirtualPosition),
    Done,
}

struct Context<'a, R>
where
    R: AsyncRead + AsyncSeek,
{
    reader: &'a mut Reader<bgzf::AsyncReader<R>>,

    chunks: vec::IntoIter<Chunk>,

    chromosome_id: ChromosomeId,
    interval: Interval,

    state: State,
}

pub fn query<R>(
    reader: &mut Reader<bgzf::AsyncReader<R>>,
    chunks: Vec<Chunk>,
    chromosome_id: ChromosomeId,
    interval: Interval,
) -> impl Stream<Item = io::Result<lazy::Record>> + '_
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    let ctx = Context {
        reader,

        chunks: chunks.into_iter(),

        chromosome_id,
        interval,

        state: State::Seek,
    };

    Box::pin(stream::try_unfold(ctx, |mut ctx| async {
        loop {
            match ctx.state {
                State::Seek => {
                    ctx.state = match ctx.chunks.next() {
                        Some(chunk) => {
                            ctx.reader.seek(chunk.start()).await?;
                            State::Read(chunk.end())
                        }
                        None => State::Done,
                    };
                }
                State::Read(chunk_end) => match next_record(ctx.reader).await? {
                    Some(record) => {
                        if ctx.reader.virtual_position() >= chunk_end {
                            ctx.state = State::Seek;
                        }

                        if intersects(&record, ctx.chromosome_id, ctx.interval)? {
                            return Ok(Some((record, ctx)));
                        }
                    }
                    None => ctx.state = State::Seek,
                },
                State::Done => return Ok(None),
            }
        }
    }))
}

async fn next_record<R>(
    reader: &mut Reader<bgzf::AsyncReader<R>>,
) -> io::Result<Option<lazy::Record>>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    let mut record = lazy::Record::default();

    reader.read_lazy_record(&mut record).await.map(|n| match n {
        0 => None,
        _ => Some(record),
    })
}

fn intersects(
    record: &lazy::Record,
    chromosome_id: usize,
    region_interval: Interval,
) -> io::Result<bool> {
    let id = record.chromosome_id();

    let start = Position::try_from(usize::from(record.position()))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let end = record.end().map(usize::from).and_then(|n| {
        Position::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    })?;

    let record_interval = Interval::from(start..=end);

    Ok(id == chromosome_id && record_interval.intersects(region_interval))
}
