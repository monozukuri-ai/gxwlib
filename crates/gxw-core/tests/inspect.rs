mod support;
use gxw_core::{GxwError, ReadLimits, inspect_bytes};
use sha2::{Digest, Sha256};

#[test]
fn inventories_and_resolves_synthetic_containers() {
    let bytes = support::fixture();
    let got = inspect_bytes(&bytes, &ReadLimits::default()).unwrap();
    assert_eq!(got.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(got.containers[0].streams.len(), 5);
    assert_eq!(got.containers[1].streams.len(), 2);
    assert_eq!(
        got.containers[1].streams[0].sha256,
        format!("{:x}", Sha256::digest(support::PROGRAM))
    );
    assert_eq!(got.containers[1].streams[1].path, ["folder", "日本語"]);
    let program = &got.logical_objects[0];
    assert_eq!(program.logical_name.as_deref(), Some("MAIN.プログラム.pou"));
    assert_eq!(program.location.as_ref().unwrap().container, ["_hdb"]);
    assert_eq!(program.location.as_ref().unwrap().path, ["7"]);
    assert!(
        got.logical_objects[1]
            .location
            .as_ref()
            .unwrap()
            .container
            .is_empty()
    );
    assert_eq!(got.diagnostics.len(), 1);
    assert_eq!(got.diagnostics[0].code, "GXW_INSPECT_ONLY");
}

#[test]
fn rejects_non_cfb_and_non_gxw() {
    assert!(matches!(
        inspect_bytes(b"not a CFB", &ReadLimits::default()),
        Err(GxwError::Format { .. })
    ));
    let ordinary = support::cfb_bytes(&[("/something", b"data")]);
    assert!(matches!(
        inspect_bytes(&ordinary, &ReadLimits::default()),
        Err(GxwError::Unsupported { .. })
    ));
    let broken = support::cfb_bytes(&[("/_hdb", b"not a CFB")]);
    assert!(
        matches!(inspect_bytes(&broken, &ReadLimits::default()), Err(GxwError::Format { context, .. }) if context == "_hdb")
    );
}

#[test]
fn rejects_truncated_and_damaged_cfb() {
    let bytes = support::fixture();
    for end in [0, 7, 511, 512, 1023, bytes.len() / 2] {
        assert!(
            inspect_bytes(&bytes[..end], &ReadLimits::default()).is_err(),
            "cut={end}"
        );
    }
    let mut damaged = bytes;
    damaged[28] = 0; // Invalid CFB byte-order marker, never a compatibility fallback.
    assert!(matches!(
        inspect_bytes(&damaged, &ReadLimits::default()),
        Err(GxwError::Format { .. })
    ));
}

#[test]
fn enforces_all_inspection_limits() {
    let bytes = support::fixture();
    for limits in [
        ReadLimits {
            max_file_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_stream_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_total_stream_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_entries: 1,
            ..Default::default()
        },
        ReadLimits {
            max_xml_bytes: 1,
            ..Default::default()
        },
        ReadLimits {
            max_xml_depth: 1,
            ..Default::default()
        },
        ReadLimits {
            max_xml_rows: 1,
            ..Default::default()
        },
    ] {
        assert!(
            matches!(
                inspect_bytes(&bytes, &limits),
                Err(GxwError::ResourceLimit { .. })
            ),
            "{limits:?}"
        );
    }
}

#[test]
fn decodes_utf16_and_xml_escapes_strictly() {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>{}",
        support::xml(&support::ROW.replace("MAIN.プログラム", "A&amp;B&#x65E5;<![CDATA[<C>]]>"))
    );
    for big_endian in [false, true] {
        let mut data = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for word in xml.encode_utf16() {
            data.extend(if big_endian {
                word.to_be_bytes()
            } else {
                word.to_le_bytes()
            });
        }
        let got = inspect_bytes(&support::project(&data), &ReadLimits::default()).unwrap();
        assert_eq!(
            got.logical_objects[0].logical_name.as_deref(),
            Some("A&B日<C>.pou")
        );
    }
}

#[test]
fn rejects_malformed_xml_and_unsupported_encodings() {
    for xml in [
        "<root>",
        "<root/><root/>",
        "<root>&bogus;</root>",
        "<root>\0</root>",
        "<root>&#1;</root>",
        "<root><x a='1' a='2'/></root>",
        "<r><D_Projectdata><iID>1</iID><iID>2</iID></D_Projectdata></r>",
    ] {
        assert!(
            matches!(
                inspect_bytes(&support::project(xml.as_bytes()), &ReadLimits::default()),
                Err(GxwError::Format { .. })
            ),
            "{xml:?}"
        );
    }
    for data in [
        b"\xff\xfe\x00".as_slice(),
        b"\xff\xfe\x00\xd8",
        b"<r>\xff</r>",
    ] {
        assert!(matches!(
            inspect_bytes(&support::project(data), &ReadLimits::default()),
            Err(GxwError::Format { .. })
        ));
    }
    for xml in [
        "<!DOCTYPE r [<!ENTITY a 'value'>]><r/>",
        "<?xml version='1.0' encoding='shift_jis'?><r/>",
    ] {
        assert!(matches!(
            inspect_bytes(&support::project(xml.as_bytes()), &ReadLimits::default()),
            Err(GxwError::Unsupported { .. })
        ));
    }
}

#[test]
fn does_not_guess_ambiguous_or_inactive_logical_objects() {
    let duplicates = support::xml(&support::ROW.repeat(2));
    let got = inspect_bytes(
        &support::project(duplicates.as_bytes()),
        &ReadLimits::default(),
    )
    .unwrap();
    assert!(got.logical_objects.iter().all(|o| o.location.is_none()));
    assert!(
        got.diagnostics
            .iter()
            .any(|d| d.code == "GXW_DUPLICATE_LOGICAL_ID")
    );
    for (row, code) in [
        (
            support::ROW.replace("<bScrapFlag>false</bScrapFlag>", ""),
            "GXW_ACTIVITY_UNKNOWN",
        ),
        (
            support::ROW.replace("<iID>7</iID>", "<iID>123</iID>"),
            "GXW_UNRESOLVED_LOGICAL_OBJECT",
        ),
    ] {
        let got = inspect_bytes(
            &support::project(support::xml(&row).as_bytes()),
            &ReadLimits::default(),
        )
        .unwrap();
        assert!(got.logical_objects[0].location.is_none());
        assert!(got.diagnostics.iter().any(|d| d.code == code));
    }
    let history = format!(
        "{}<diffgr:before>{}</diffgr:before>{}{}",
        support::XML_PREFIX,
        support::ROW,
        support::ROW.replace("false", "true"),
        support::XML_SUFFIX
    );
    let got = inspect_bytes(
        &support::project(history.as_bytes()),
        &ReadLimits::default(),
    )
    .unwrap();
    assert!(got.logical_objects[0].historical);
    assert_eq!(got.logical_objects[1].scrap, Some(true));
    assert!(got.logical_objects.iter().all(|o| o.location.is_none()));
}
