use std::fs;
use std::io::{self, Write};
use std::path::Path;

use cpt::fs::archive::{UnpackCommand, decompress, resolve_unpack_command};
use flate2::{Compression, write::GzEncoder};

fn gzip_member(bytes: u64) -> Vec<u8> {
    use std::io::Read;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    io::copy(&mut io::repeat(0).take(bytes), &mut encoder).unwrap();
    encoder.finish().unwrap()
}

async fn unpack(archive: &Path, dest: &Path) -> Result<(), String> {
    match resolve_unpack_command(archive, dest)? {
        UnpackCommand::Decompress {
            program,
            archive,
            output,
        } => decompress(program, archive, output).await,
        _ => panic!("expected a single-file decoder"),
    }
}

#[tokio::test]
async fn gzip_expansion_limit_counts_concatenated_members_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let archive = root.path().join("data.gz");
    let dest = root.path().join("out");
    fs::create_dir(&dest).unwrap();
    let limit = 8 * 1024 * 1024;
    let member = gzip_member(limit / 2);
    fs::write(&archive, [member.as_slice(), member.as_slice()].concat()).unwrap();
    assert!(fs::metadata(&archive).unwrap().len() * 200 < limit);
    unpack(&archive, &dest).await.unwrap();
    let output = dest.join("data");
    assert_eq!(fs::metadata(&output).unwrap().len(), limit);
    fs::remove_file(output).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&archive)
        .unwrap()
        .write_all(&gzip_member(1))
        .unwrap();
    assert!(
        unpack(&archive, &dest)
            .await
            .unwrap_err()
            .contains("byte limit")
    );
    assert_eq!(fs::read_dir(dest).unwrap().count(), 0);
}
