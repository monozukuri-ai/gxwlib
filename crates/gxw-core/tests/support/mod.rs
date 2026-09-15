#![allow(dead_code)]
use std::io::{Cursor, Write};

pub const PROGRAM: &[u8] = b"opaque synthetic program; not PLC instructions";
pub const ROW: &str = "<D_Projectdata><iID>7</iID><szName>MAIN.プログラム.pou</szName><bScrapFlag>false</bScrapFlag></D_Projectdata>";
pub const XML_PREFIX: &str =
    "<diffgr:diffgram xmlns:diffgr=\"urn:schemas-microsoft-com:xml-diffgram-v1\"><DSPROJECTDATA>";
pub const XML_SUFFIX: &str = "</DSPROJECTDATA></diffgr:diffgram>";

pub fn xml(rows: &str) -> String {
    format!("{XML_PREFIX}{rows}{XML_SUFFIX}")
}

pub fn cfb_bytes(streams: &[(&str, &[u8])]) -> Vec<u8> {
    let mut file =
        cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new())).unwrap();
    for (path, data) in streams {
        let parent = std::path::Path::new(path).parent().unwrap();
        if parent != std::path::Path::new("/") {
            file.create_storage_all(parent).unwrap();
        }
        file.create_stream(path).unwrap().write_all(data).unwrap();
    }
    file.into_inner().into_inner()
}

pub fn project(metadata: &[u8]) -> Vec<u8> {
    let inner = cfb_bytes(&[("/7", PROGRAM), ("/folder/日本語", b"nested stream")]);
    cfb_bytes(&[
        ("/_hdb", &inner),
        ("/projectdatalist.xml", metadata),
        (
            "/projectlist.xml",
            b"<DSPROJECT><D_Project><iID>1</iID><szName>synthetic</szName></D_Project></DSPROJECT>",
        ),
        ("/Project.gd2", b""),
        ("/user.xml", b"opaque binary, not XML"),
    ])
}

pub fn fixture() -> Vec<u8> {
    let rows = format!(
        "{ROW}<D_Projectdata><iID>99</iID><szName>Project.gd2</szName><bScrapFlag>false</bScrapFlag></D_Projectdata>"
    );
    project(xml(&rows).as_bytes())
}

pub const RAW_BODY: &[u8] = &[3, 0xee, 3, 4, 0x9c, 1, 4, 3, 0x34, 3];

/// Construct our own framing case from the observed layout, never vendor payloads.
pub fn pou(body: &[u8]) -> Vec<u8> {
    let mut data = vec![0; 79];
    data[0] = 1;
    data[6] = 1;
    data[20] = 2;
    data[24] = 1;
    data[28] = 1;
    data[54] = 1;
    data[67] = 12;
    data[75..79].fill(0xff);
    for offset in [55, 59] {
        data[offset..offset + 4].copy_from_slice(&((body.len() + 20) as u32).to_le_bytes());
    }
    data.extend_from_slice(body);
    data.extend_from_slice(&[0; 20]);
    data
}

pub fn raw_row(id: &str, name: &str) -> String {
    format!(
        "<D_Projectdata><iID>{id}</iID><szName>{name}</szName><bScrapFlag>false</bScrapFlag><ucFolderType>7</ucFolderType><ucFileType>2</ucFileType></D_Projectdata>"
    )
}

pub fn raw_project(metadata: &[u8], pous: &[(&str, &[u8])]) -> Vec<u8> {
    let inner = cfb_bytes(pous);
    cfb_bytes(&[
        ("/_hdb", &inner),
        ("/projectdatalist.xml", metadata),
        ("/projectlist.xml", b"<DSPROJECT/>"),
    ])
}

pub fn raw_fixture() -> Vec<u8> {
    raw_project(
        xml(&raw_row("7", "MAIN.プログラム.pou")).as_bytes(),
        &[("/7", &pou(RAW_BODY))],
    )
}
