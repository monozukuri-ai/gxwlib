use crate::{ContainerIndex, GxwError, ReadLimits, StreamInfo};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::Component,
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct Budget {
    entries: u64,
    stream_bytes: u64,
}

pub(crate) fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub(crate) type Payloads = BTreeMap<Vec<String>, Arc<[u8]>>;

pub(crate) fn inspect(
    bytes: &[u8],
    path: &[String],
    limits: &ReadLimits,
    budget: &mut Budget,
    retain_all: bool,
) -> Result<(ContainerIndex, Payloads), GxwError> {
    let context = if path.is_empty() {
        "outer CFB".into()
    } else {
        path.join("/")
    };
    let mut cfb = cfb::OpenOptions::new()
        .strict()
        .max_buffer_size(64 << 10)
        .open_with(Cursor::new(bytes))
        .map_err(|e| GxwError::format(&context, e))?;
    let mut entries = Vec::new();
    for entry in cfb.walk() {
        budget.entries = budget
            .entries
            .checked_add(1)
            .ok_or_else(|| GxwError::format(&context, "entry count overflow"))?;
        limits.check("CFB entries", budget.entries, limits.max_entries)?;
        if entry.is_stream() {
            entries.push(entry);
        }
    }
    let mut streams = Vec::new();
    let mut payloads = BTreeMap::new();
    for entry in entries {
        let names: Vec<String> = entry
            .path()
            .components()
            .filter_map(|c| match c {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        limits.check("stream bytes", entry.len(), limits.max_stream_bytes)?;
        budget.stream_bytes = budget
            .stream_bytes
            .checked_add(entry.len())
            .ok_or_else(|| GxwError::format(&context, "stream size overflow"))?;
        limits.check(
            "total stream bytes",
            budget.stream_bytes,
            limits.max_total_stream_bytes,
        )?;
        let metadata = path.is_empty()
            && names.len() == 1
            && matches!(
                names[0].as_str(),
                "_hdb" | "projectdatalist.xml" | "projectlist.xml"
            );
        let keep = retain_all || metadata;
        if metadata && names[0].ends_with(".xml") {
            limits.check("XML bytes", entry.len(), limits.max_xml_bytes)?;
        }
        // Hash every stream, but retain only the payloads required for inspection.
        let mut stream = cfb
            .open_stream(entry.path())
            .map_err(|e| GxwError::format(&context, e))?;
        let mut hasher = Sha256::new();
        let mut retained = Vec::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let n = stream
                .read(&mut buffer)
                .map_err(|e| GxwError::format(format!("{context}/{}", names.join("/")), e))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > entry.len() {
                return Err(GxwError::format(&context, "stream exceeds declared length"));
            }
            hasher.update(&buffer[..n]);
            if keep {
                retained.extend_from_slice(&buffer[..n]);
            }
        }
        if total != entry.len() {
            return Err(GxwError::format(&context, "truncated stream"));
        }
        if keep {
            payloads.insert(names.clone(), Arc::from(retained));
        }
        streams.push(StreamInfo {
            path: names,
            size: total,
            sha256: format!("{:x}", hasher.finalize()),
        });
    }
    streams.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((
        ContainerIndex {
            path: path.to_vec(),
            cfb_version: match cfb.version() {
                cfb::Version::V3 => 3,
                cfb::Version::V4 => 4,
            },
            streams,
        },
        payloads,
    ))
}
