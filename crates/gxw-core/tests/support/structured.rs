//! Independently authored binary controls. No native/manufacturer bytes copied.
pub fn uint(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
pub fn string(out: &mut Vec<u8>, value: &str) {
    let units: Vec<_> = value.encode_utf16().chain([0]).collect();
    uint(out, units.len() as u32);
    for u in units {
        out.extend(u.to_le_bytes());
    }
}
pub fn header() -> Vec<u8> {
    let mut v = vec![0; 54];
    for i in [0, 6, 12, 24, 28] {
        v[i] = 1;
    }
    v[14] = 10;
    v[20] = 2;
    v
}
pub fn node(
    kind: u32,
    symbol: &str,
    type_name: &str,
    bounds: [u32; 4],
    ports: &[(u32, u32, u32)],
) -> Vec<u8> {
    let mut v = vec![0; 4];
    uint(&mut v, 1);
    uint(&mut v, kind);
    string(&mut v, symbol);
    if kind == 2 {
        string(&mut v, type_name);
    } else {
        uint(&mut v, 1);
        v.extend([0; 2]);
    }
    for b in bounds {
        uint(&mut v, b);
    }
    uint(&mut v, ports.len() as u32);
    for (k, x, y) in ports {
        for a in [16, *k, *x, *y] {
            uint(&mut v, a);
        }
    }
    let n = v.len() as u32;
    v[..4].copy_from_slice(&n.to_le_bytes());
    v
}
pub fn wire(start: (u32, u32), end: (u32, u32)) -> Vec<u8> {
    let mut v = vec![];
    for a in [44, 2, 0, 1] {
        uint(&mut v, a);
    }
    v.extend([0, 0, 1, 0]);
    for a in [0, start.0, start.1, end.0, end.1, 0] {
        uint(&mut v, a);
    }
    v
}
pub fn pou(blocks: &[Vec<Vec<u8>>]) -> Vec<u8> {
    let mut v = header();
    v.push(0xd0);
    v.extend([0; 8]);
    uint(&mut v, 1);
    uint(&mut v, blocks.len() as u32);
    for records in blocks {
        uint(
            &mut v,
            24 + records.iter().map(|r| r.len() as u32).sum::<u32>(),
        );
        v.extend([1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
        uint(&mut v, 12);
        uint(&mut v, records.len() as u32);
        for r in records {
            v.extend(r);
        }
    }
    v.extend([0; 24]);
    let n = (v.len() - 83) as u32;
    for at in [55, 59] {
        v[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    v
}
pub fn local() -> Vec<u8> {
    let mut v = header();
    string(&mut v, "MAIN");
    v.extend([0; 20]);
    uint(&mut v, 1);
    string(&mut v, "積算器");
    string(&mut v, "MY_FB");
    uint(&mut v, 1);
    string(&mut v, "");
    string(&mut v, "");
    uint(&mut v, 0);
    string(&mut v, "");
    string(&mut v, "");
    uint(&mut v, 7);
    string(&mut v, "<script>& comment");
    uint(&mut v, 0);
    uint(&mut v, 15);
    string(&mut v, "MY_FB");
    v.extend([0; 16]);
    v.extend([255, 0, 0, 0, 0, 0, 0, 0]);
    v
}
pub fn fixture() -> Vec<u8> {
    let first = vec![
        node(3, "X0", "", [2, 1, 4, 3], &[(3, 0, 1), (2, 2, 1)]),
        node(2, "積算器", "MY_FB", [6, 1, 10, 4], &[(3, 0, 1), (0, 4, 1)]),
        wire((4, 2), (6, 2)),
    ];
    let second = vec![
        node(4, "X1<&", "", [2, 1, 4, 3], &[(3, 0, 1), (2, 2, 1)]),
        node(5, "Y0", "", [6, 1, 8, 3], &[(3, 0, 1), (0, 2, 1)]),
        wire((4, 2), (6, 2)),
    ];
    let p = pou(&[first, second]);
    let rows = format!(
        "{}{}",
        crate::support::raw_row("7", "MAIN.Program.pou"),
        "<D_Projectdata><iID>8</iID><szName>MAIN.Labels.lh</szName><bScrapFlag>false</bScrapFlag></D_Projectdata>"
    );
    crate::support::raw_project(
        crate::support::xml(&rows).as_bytes(),
        &[("/7", &p), ("/8", &local())],
    )
}
