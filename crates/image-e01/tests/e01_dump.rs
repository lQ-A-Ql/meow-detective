#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn dump_section_walk() {
    let p = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 dump tests"));
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(&p).unwrap();
    let flen = f.seek(SeekFrom::End(0)).unwrap();
    f.seek(SeekFrom::Start(0)).unwrap();

    // Skip 13-byte file header
    f.seek(SeekFrom::Start(13)).unwrap();

    let mut off = 13u64;
    for _ in 0..20 {
        f.seek(SeekFrom::Start(off)).unwrap();
        let mut desc = [0u8; 76];
        if f.read_exact(&mut desc).is_err() {
            eprintln!("read error at {}", off);
            break;
        }

        let stype = String::from_utf8_lossy(&desc[0..16])
            .trim_end_matches('\0')
            .to_string();
        let next = u64::from_le_bytes(desc[16..24].try_into().unwrap());
        let ssize = u64::from_le_bytes(desc[24..32].try_into().unwrap());

        eprintln!(
            "section '{}' at {}: next={}, size={}",
            stype, off, next, ssize
        );
        eprintln!("  desc[0..32]: {:02X?}", &desc[0..32]);

        if stype == "done" || next == 0 || next >= flen || next == off {
            break;
        }
        off = next;
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn dump_first_table_bytes() {
    let p = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 dump tests"));
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(&p).unwrap();
    let table_off = 258681683u64;
    f.seek(SeekFrom::Start(table_off)).unwrap();

    let mut desc = [0u8; 76];
    f.read_exact(&mut desc).unwrap();
    let mut content = [0u8; 64];
    f.read_exact(&mut content).unwrap();

    eprintln!("table desc[0..32]: {:02X?}", &desc[0..32]);
    eprintln!("table content[0..64]: {:02X?}", &content);
}
