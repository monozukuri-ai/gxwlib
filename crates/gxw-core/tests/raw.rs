mod support;
use gxw_core::{FramingStatus, GxwError, ParsedProject, ReadLimits, SourceSpan, parse_bytes};
use std::sync::Arc;

fn parse(data: Vec<u8>) -> ParsedProject {
    parse_bytes(Arc::from(data), &ReadLimits::default()).unwrap()
}
fn one(pou: &[u8]) -> ParsedProject {
    parse(support::raw_project(
        support::xml(&support::raw_row("7", "MAIN.pou")).as_bytes(),
        &[("/7", pou)],
    ))
}

#[test]
fn raw_unknown_tokens_have_checked_source_ranges() {
    let bytes = support::raw_fixture();
    let project = parse(bytes);
    assert!(project.framing_complete());
    let p = &project.programs[0];
    assert_eq!(p.profile.as_deref(), Some("simple-le-observed-v1"));
    assert_eq!(p.tokens.len(), 3);
    assert_eq!(
        project.source_bytes(&p.tokens[0].source).unwrap(),
        [3, 0xee, 3]
    );
    assert_eq!(
        p.tokens.iter().map(|t| t.source.offset).collect::<Vec<_>>(),
        [79, 82, 86]
    );
    assert_eq!(p.trailer.as_ref().unwrap().length, 20);
    assert_eq!(
        project.source_bytes(p.trailer.as_ref().unwrap()).unwrap(),
        [0; 20]
    );
    let full = project.source_bytes(p.source.as_ref().unwrap()).unwrap();
    let pieces = [
        &p.opaque_regions[0].source,
        &p.tokens[0].source,
        &p.tokens[1].source,
        &p.tokens[2].source,
        p.trailer.as_ref().unwrap(),
    ];
    let rebuilt: Vec<u8> = pieces
        .into_iter()
        .flat_map(|s| project.source_bytes(s).unwrap())
        .copied()
        .collect();
    assert_eq!(rebuilt, full);
    assert_eq!(
        project.source_bytes(&p.metadata_source).unwrap(),
        support::raw_row("7", "MAIN.プログラム.pou").as_bytes()
    );
    let json: serde_json::Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
    assert_eq!(json["semantic_status"], "not_decoded");
    assert!(json["cpu_model"].is_null());
    assert!(json["execution_order"].is_null());
    assert_eq!(json["programs"][0]["tokens"][0]["data_hex"], "03 ee 03");
    assert!(
        project
            .source_bytes(&SourceSpan {
                source_id: u32::MAX,
                offset: 0,
                length: 1
            })
            .is_none()
    );
    assert!(
        project
            .source_bytes(&SourceSpan {
                source_id: 0,
                offset: u64::MAX,
                length: 1
            })
            .is_none()
    );
}

#[test]
fn rejects_every_truncation_without_inventing_a_boundary() {
    let data = support::pou(support::RAW_BODY);
    for cut in 0..data.len() {
        let project = one(&data[..cut]);
        assert!(!project.framing_complete(), "cut={cut}");
        assert!(project.programs[0].tokens.is_empty(), "cut={cut}");
        assert!(!project.programs[0].diagnostics.is_empty());
        assert_eq!(
            project
                .source_bytes(project.programs[0].source.as_ref().unwrap())
                .unwrap(),
            &data[..cut]
        );
    }
}

#[test]
fn validates_lengths_trailers_profile_and_terminal_record() {
    let good = support::pou(support::RAW_BODY);
    let mut cases = Vec::new();
    let mut mismatch = good.clone();
    mismatch[55] += 1;
    cases.push((mismatch, "GXW_POU_LENGTH_MISMATCH"));
    let mut short = good.clone();
    short[55..59].copy_from_slice(&19u32.to_le_bytes());
    short[59..63].copy_from_slice(&19u32.to_le_bytes());
    cases.push((short, "GXW_POU_LENGTH_INVALID"));
    let mut huge = good.clone();
    huge[55..63].fill(0xff);
    cases.push((huge, "GXW_POU_SIZE_MISMATCH"));
    let mut extra = good.clone();
    extra.extend_from_slice(&[0; 4]);
    cases.push((extra, "GXW_POU_SIZE_MISMATCH"));
    let mut trailer = good.clone();
    *trailer.last_mut().unwrap() = 1;
    cases.push((trailer, "GXW_POU_TRAILER_UNSUPPORTED"));
    let mut header = good.clone();
    header[20] = 3;
    cases.push((header, "GXW_POU_PROFILE_UNSUPPORTED"));
    cases.push((support::pou(&[3, 0xee, 3]), "GXW_POU_TERMINATOR_MISSING"));
    cases.push((support::pou(&[]), "GXW_POU_TERMINATOR_MISSING"));
    for (bytes, code) in cases {
        let project = one(&bytes);
        assert!(!project.framing_complete(), "{code}");
        assert!(project.diagnostics.iter().any(|d| d.code == code), "{code}");
    }
}

#[test]
fn invalid_token_preserves_prefix_and_opaque_remainder_without_resync() {
    for invalid in [
        vec![0, 3, 0x34, 3],
        vec![2, 2, 3, 0x34, 3],
        vec![255, 1, 3, 0x34, 3],
        vec![4, 0x99, 0, 5, 3, 0x34, 3],
    ] {
        let body = [vec![3, 0xee, 3], invalid].concat();
        let project = one(&support::pou(&body));
        let p = &project.programs[0];
        assert_eq!(p.framing_status, FramingStatus::Partial);
        assert_eq!(p.tokens.len(), 1);
        let remainder = &p.opaque_regions[1].source;
        assert_eq!(remainder.offset, 82);
        assert_eq!(project.source_bytes(remainder).unwrap(), &body[3..]);
    }
}

#[test]
fn profile_uses_metadata_and_content_with_names_only_as_candidates() {
    let pou = support::pou(support::RAW_BODY);
    for name in ["RENAMED.プログラム.pou", "renamed.bin"] {
        let project = parse(support::raw_project(
            support::xml(&support::raw_row("203", name)).as_bytes(),
            &[("/203", &pou)],
        ));
        assert!(project.framing_complete());
        assert_eq!(project.programs[0].logical_name.as_deref(), Some(name));
    }
    let project = parse(support::raw_project(
        support::xml(support::ROW).as_bytes(),
        &[("/7", &pou)],
    ));
    assert!(!project.framing_complete());
    assert_eq!(
        project.programs[0].diagnostics[0].code,
        "GXW_POU_METADATA_UNSUPPORTED"
    );
    let mut alternate = pou.clone();
    alternate[12..20].copy_from_slice(&[1, 0, 10, 0, 0, 0, 0, 0]);
    alternate[63] = 1;
    alternate[34..50].fill(0x12); // Preserved opaque fields, not a guessed timestamp.
    assert!(one(&alternate).framing_complete());
    alternate[63] = 0;
    assert!(!one(&alternate).framing_complete()); // Unobserved header-field combination.
    assert!(!one(b"FX3G MAIN.Program.pou").framing_complete());
}

#[test]
fn duplicate_ids_history_and_duplicate_names_do_not_merge_programs() {
    let pou = support::pou(support::RAW_BODY);
    let row = support::raw_row("7", "SAME.pou");
    let duplicates = parse(support::raw_project(
        support::xml(&row.repeat(2)).as_bytes(),
        &[("/7", &pou)],
    ));
    assert_eq!(duplicates.programs.len(), 2);
    assert!(duplicates.programs.iter().all(|p| p.source.is_none()));
    let rows = format!("{row}{}", support::raw_row("8", "SAME.pou"));
    let duplicate_names = parse(support::raw_project(
        support::xml(&rows).as_bytes(),
        &[("/7", &pou), ("/8", &pou)],
    ));
    assert_eq!(duplicate_names.programs.len(), 2);
    assert!(duplicate_names.framing_complete());
    let history = format!(
        "<d:diffgram xmlns:d='urn:schemas-microsoft-com:xml-diffgram-v1'><DSPROJECTDATA>{row}</DSPROJECTDATA><d:before>{row}</d:before></d:diffgram>"
    );
    let project = parse(support::raw_project(history.as_bytes(), &[("/7", &pou)]));
    assert_eq!(project.programs.len(), 1);
    assert!(project.index.logical_objects[1].historical);
    let scrap = row.replace("false", "true");
    assert!(
        parse(support::raw_project(
            support::xml(&scrap).as_bytes(),
            &[("/7", &pou)]
        ))
        .programs
        .is_empty()
    );
}

#[test]
fn enforces_project_wide_token_limit() {
    let pou = support::pou(support::RAW_BODY);
    let rows = format!(
        "{}{}",
        support::raw_row("7", "A.pou"),
        support::raw_row("8", "B.pou")
    );
    let data = support::raw_project(
        support::xml(&rows).as_bytes(),
        &[("/7", &pou), ("/8", &pou)],
    );
    let limits = ReadLimits {
        max_tokens: 5,
        ..Default::default()
    };
    assert!(
        matches!(parse_bytes(data.into(), &limits), Err(GxwError::ResourceLimit { resource, .. }) if resource == "tokens")
    );
}

#[test]
fn metadata_source_offsets_are_original_utf16_bytes_with_bom_and_surrogates() {
    let pou = support::pou(support::RAW_BODY);
    let row = support::raw_row("7", "日本語😀.pou");
    let xml = format!(
        "<?xml version='1.0' encoding='UTF-16'?><!--😀-->{}",
        support::xml(&row)
    );
    for big in [false, true] {
        let mut bytes = if big {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        bytes.extend(xml.encode_utf16().flat_map(|c| {
            if big {
                c.to_be_bytes()
            } else {
                c.to_le_bytes()
            }
        }));
        let project = parse(support::raw_project(&bytes, &[("/7", &pou)]));
        let expected: Vec<u8> = row
            .encode_utf16()
            .flat_map(|c| {
                if big {
                    c.to_be_bytes()
                } else {
                    c.to_le_bytes()
                }
            })
            .collect();
        assert_eq!(
            project
                .source_bytes(&project.programs[0].metadata_source)
                .unwrap(),
            expected
        );
        assert!(project.framing_complete());
    }
}

#[test]
fn foreign_namespaces_are_retained_without_assigning_schema_semantics() {
    let pou = support::pou(support::RAW_BODY);
    let xml = support::xml(&support::raw_row("7", "A.pou"))
        .replace("<DSPROJECTDATA>", "<DSPROJECTDATA xmlns='urn:unknown'>");
    let project = parse(support::raw_project(xml.as_bytes(), &[("/7", &pou)]));
    assert!(!project.framing_complete());
    assert!(project.programs[0].source.is_none());
    assert!(
        project
            .diagnostics
            .iter()
            .any(|d| d.code == "GXW_METADATA_SCHEMA_UNSUPPORTED")
    );
}

#[test]
fn rejects_unbound_namespace_prefixes_and_invalid_attribute_characters() {
    for attribute in ["bad:name='1'", "xmlnsfake:name='1'", "valid='&#1;'"] {
        let row = support::raw_row("7", "A.pou")
            .replace("<D_Projectdata>", &format!("<D_Projectdata {attribute}>"));
        let data = support::raw_project(
            support::xml(&row).as_bytes(),
            &[("/7", &support::pou(support::RAW_BODY))],
        );
        assert!(
            matches!(
                parse_bytes(data.into(), &ReadLimits::default()),
                Err(GxwError::Format { .. })
            ),
            "{attribute}"
        );
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn cycle_directory(bytes: &mut [u8]) {
    let first_dir = u32_at(bytes, 48);
    let fat = u32_at(bytes, 76) as usize;
    let at = (fat + 1) * 512 + first_dir as usize * 4;
    bytes[at..at + 4].copy_from_slice(&first_dir.to_le_bytes());
}

#[test]
fn rejects_cyclic_outer_and_inner_fat_chains_and_missing_hdb() {
    let mut outer = support::raw_fixture();
    cycle_directory(&mut outer);
    assert!(matches!(
        parse_bytes(outer.into(), &ReadLimits::default()),
        Err(GxwError::Format { .. })
    ));
    let mut inner = support::cfb_bytes(&[("/7", &support::pou(support::RAW_BODY))]);
    cycle_directory(&mut inner);
    let outer = support::cfb_bytes(&[("/_hdb", &inner), ("/projectdatalist.xml", b"<r/>")]);
    assert!(
        matches!(parse_bytes(outer.into(), &ReadLimits::default()), Err(GxwError::Format { context, .. }) if context == "_hdb")
    );
    let missing = support::cfb_bytes(&[("/projectdatalist.xml", b"<r/>")]);
    assert!(matches!(
        parse_bytes(missing.into(), &ReadLimits::default()),
        Err(GxwError::Unsupported { .. })
    ));
}
