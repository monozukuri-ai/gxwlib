#[path = "support/structured.rs"]
mod control;
mod support;
#[test]
fn explicit_cpu_radices_never_assume_cpu_ranges() {
    use devices::*;
    assert_eq!(
        parse_device_address("X10", CpuFamily::Fx).unwrap().address,
        8
    );
    for family in [CpuFamily::Q, CpuFamily::L] {
        let a = parse_device_address("x01f", family).unwrap();
        assert_eq!((a.address, a.radix), (31, 16));
        assert_eq!(a.canonical, "X1F");
        assert_eq!(a.range_status, "not_checked");
        assert_eq!(parse_device_address("T200", family).unwrap().radix, 10);
        assert!(parse_device_address("DFF", family).is_err());
    }
    for text in ["X8", "X-1", "X42949672960", "X1Z0", "局所変数"] {
        assert!(parse_device_address(text, CpuFamily::Fx).is_err());
    }
}
use gxw_core::{structured::*, *};
use std::sync::Arc;
fn parse(pou: &[u8]) -> Arc<ParsedProject> {
    Arc::new(
        parse_bytes(
            support::raw_project(
                support::xml(&support::raw_row("7", "MAIN.Program.pou")).as_bytes(),
                &[("/7", pou)],
            )
            .into(),
            &ReadLimits::default(),
        )
        .unwrap(),
    )
}
fn decode(pou: &[u8]) -> Result<StructuredProgram, GxwError> {
    decode_structured(parse(pou), 0, &StructuredLimits::default())
}

#[test]
fn source_owned_geometry_declarations_and_common_ir() {
    let bytes = control::fixture();
    assert_eq!(
        bytes,
        include_bytes!("../../../tests/fixtures/structured.gxw")
    );
    let project = Arc::new(parse_bytes(bytes.into(), &ReadLimits::default()).unwrap());
    let p = decode_structured(project.clone(), 0, &StructuredLimits::default()).unwrap();
    assert!(p.structure_complete);
    assert_eq!(p.blocks.len(), 2);
    assert_eq!(p.semantic_status, "not_evaluated");
    let b = &p.blocks[0];
    assert_eq!(b.nodes[1].type_name.as_deref(), Some("MY_FB"));
    assert_eq!(b.nodes[1].symbol, "積算器");
    assert_eq!(
        b.nets[1].ports,
        [PortRef { node: 0, port: 1 }, PortRef { node: 1, port: 0 }]
    );
    assert_eq!(b.nets[1].wires, [0]);
    let d = decode_declarations(project.clone(), 1, &StructuredLimits::default()).unwrap();
    assert!(d.structure_complete);
    assert_eq!(d.owner_name.as_deref(), Some("MAIN"));
    assert_eq!(d.rows[0].type_code, 15);
    assert!(matches!(
        decode_ir(
            project.clone(),
            0,
            DeviceProfile::Fx,
            &StructuredLimits::default()
        )
        .unwrap(),
        ProgramIr::Structured(_)
    ));
    drop(project);
    assert_eq!(
        p.project()
            .source_bytes(&b.nodes[1].ports[0].source)
            .unwrap()
            .len(),
        16
    );
}
#[test]
fn topology_junctions_crossings_and_blocks() {
    let cross = vec![
        control::wire((0, 5), (10, 5)),
        control::wire((5, 0), (5, 10)),
    ];
    let p = decode(&control::pou(&[cross.clone(), cross.clone()])).unwrap();
    assert_eq!(p.blocks.len(), 2);
    assert!(p.blocks.iter().all(|b| b.nets.len() == 2));
    let mut junction = cross.clone();
    junction.push(control::wire((5, 5), (9, 5)));
    assert_eq!(
        decode(&control::pou(&[junction])).unwrap().blocks[0]
            .nets
            .len(),
        1
    );
    let mut port = cross;
    port.push(control::node(
        3,
        "X0",
        "",
        [5, 5, 7, 7],
        &[(3, 0, 0), (2, 0, 0)],
    ));
    assert_eq!(
        decode(&control::pou(&[port])).unwrap().blocks[0].nets.len(),
        1
    );
}
#[test]
fn opaque_records_flags_and_diagonal_wires_fail_closed() {
    for raw in [
        vec![8, 0, 0, 0, 99, 0, 0, 0],
        vec![12, 0, 0, 0, 1, 0, 0, 0, 99, 0, 0, 0],
        control::wire((0, 0), (5, 5)),
    ] {
        let p = decode(&control::pou(&[vec![raw]])).unwrap();
        assert!(!p.structure_complete);
        assert!(p.blocks[0].nets.is_empty());
        assert!(!p.diagnostics.is_empty());
    }
    let mut raw = control::wire((0, 0), (0, 5));
    raw[8] = 9;
    assert!(
        !decode(&control::pou(&[vec![raw]]))
            .unwrap()
            .structure_complete
    );
}
#[test]
fn malformed_lengths_unicode_overflow_and_all_truncations() {
    let node = control::node(3, "X0", "", [2, 1, 4, 3], &[(3, 0, 1)]);
    let good = control::pou(&[vec![node.clone()]]);
    for cut in 0..good.len() {
        assert!(decode(&good[..cut]).is_err(), "cut={cut}");
    }
    for at in [0, 54, 55, 59, 63, 67, 71, 75, 91, 95] {
        let mut bad = good.clone();
        bad[at] = 255;
        assert!(decode(&bad).is_err(), "offset={at}");
    }
    for (at, bytes) in [(16, vec![0, 0xd8]), (20, vec![1, 0]), (12, vec![255; 4])] {
        let mut n = node.clone();
        n[at..at + bytes.len()].copy_from_slice(&bytes);
        assert!(decode(&control::pou(&[vec![n]])).is_err());
    }
    let n = control::node(3, "X0", "", [u32::MAX, 0, u32::MAX, 5], &[(3, 1, 0)]);
    assert!(decode(&control::pou(&[vec![n]])).is_err());
}
#[test]
fn bounded_records_strings_ports_and_connection_work() {
    let bytes = control::pou(&[vec![
        control::node(3, "X0", "", [0, 0, 2, 2], &[(3, 0, 0)]),
        control::wire((0, 0), (4, 0)),
    ]]);
    for l in [
        StructuredLimits {
            max_blocks: 0,
            ..Default::default()
        },
        StructuredLimits {
            max_records: 1,
            ..Default::default()
        },
        StructuredLimits {
            max_ports: 0,
            ..Default::default()
        },
        StructuredLimits {
            max_string_units: 1,
            ..Default::default()
        },
        StructuredLimits {
            max_connection_checks: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            decode_structured(parse(&bytes), 0, &l),
            Err(GxwError::ResourceLimit { .. })
        ));
    }
}
#[test]
fn rendering_escapes_and_preserves_every_record_source() {
    let project = Arc::new(parse_bytes(control::fixture().into(), &ReadLimits::default()).unwrap());
    let p = Arc::new(decode_structured(project.clone(), 0, &StructuredLimits::default()).unwrap());
    let doc = structured::render::render_structured(p.clone(), &RenderOptions::default()).unwrap();
    assert_eq!(doc.elements.len(), 14);
    assert!(doc.svg.contains("X1&lt;&amp;"));
    assert_eq!(doc.layout, "stored_block_geometry_stacked");
    for e in &doc.elements {
        assert!(project.source_bytes(&e.source).is_some());
    }
    assert!(doc.to_html().unwrap().contains("\\u003c"));
    assert!(
        structured::render::render_structured(
            p.clone(),
            &RenderOptions {
                max_elements: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        structured::render::render_structured(
            p,
            &RenderOptions {
                max_output_bytes: 100,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn declaration_trailer_limits_and_historical_selection() {
    let project = Arc::new(parse_bytes(control::fixture().into(), &ReadLimits::default()).unwrap());
    assert!(matches!(
        decode_declarations(
            project.clone(),
            1,
            &StructuredLimits {
                max_declarations: 0,
                ..Default::default()
            }
        ),
        Err(GxwError::ResourceLimit { .. })
    ));
    assert!(decode_declarations(project, 0, &StructuredLimits::default()).is_err());
    let mut label = control::local();
    label.push(1);
    let p=Arc::new(parse_bytes(support::raw_project(support::xml("<D_Projectdata><iID>8</iID><szName>MAIN.Labels.lh</szName><bScrapFlag>false</bScrapFlag></D_Projectdata>").as_bytes(),&[("/8",&label)]).into(),&ReadLimits::default()).unwrap());
    let d = decode_declarations(p, 0, &StructuredLimits::default()).unwrap();
    assert!(!d.structure_complete);
    assert_eq!(d.trailer.length, 25);
}
