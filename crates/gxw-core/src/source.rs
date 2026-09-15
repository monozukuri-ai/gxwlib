use crate::StreamLocation;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

/// A checked range in a logical CFB stream, never a physical file offset.
/// source_id is scoped to one ParsedProject; pair it with that project's SHA-256.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub source_id: u32,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceInfo {
    pub id: u32,
    pub location: StreamLocation,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug)]
pub(crate) struct SourceStore {
    pub infos: Vec<SourceInfo>,
    buffers: Vec<Arc<[u8]>>,
    locations: BTreeMap<StreamLocation, u32>,
}

impl SourceStore {
    pub fn new(streams: crate::RetainedStreams) -> Self {
        let (infos, buffers): (Vec<SourceInfo>, Vec<Arc<[u8]>>) = streams
            .into_iter()
            .enumerate()
            .map(|(id, (location, data))| {
                (
                    SourceInfo {
                        id: id as u32,
                        location,
                        size: data.len() as u64,
                        sha256: crate::container::digest(&data),
                    },
                    data,
                )
            })
            .unzip();
        let locations = infos.iter().map(|s| (s.location.clone(), s.id)).collect();
        Self {
            infos,
            buffers,
            locations,
        }
    }

    pub fn find(&self, location: &StreamLocation) -> Option<u32> {
        self.locations.get(location).copied()
    }

    pub fn span(&self, id: u32, start: usize, end: usize) -> Option<SourceSpan> {
        self.buffers.get(id as usize)?.get(start..end)?;
        Some(SourceSpan {
            source_id: id,
            offset: start as u64,
            length: (end - start) as u64,
        })
    }

    pub fn bytes(&self, span: &SourceSpan) -> Option<&[u8]> {
        let start = usize::try_from(span.offset).ok()?;
        let end = usize::try_from(span.offset.checked_add(span.length)?).ok()?;
        self.buffers.get(span.source_id as usize)?.get(start..end)
    }

    pub fn whole(&self, id: u32) -> SourceSpan {
        SourceSpan {
            source_id: id,
            offset: 0,
            length: self.infos[id as usize].size,
        }
    }
}
