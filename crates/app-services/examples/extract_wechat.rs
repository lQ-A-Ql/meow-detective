//! Scratch extractor: pull the WeChat ground-truth files from the private
//! E01 into an extract directory for wechat_realdata_validation.
//! Usage: cargo run -p app-services --example extract_wechat -- <image.E01> <offset> <out_dir>

use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use std::io::{Read, Write};
use std::path::Path;

const FILES: &[(&str, &str)] = &[
    (
        "Program Files\\Tencent\\Weixin\\4.1.8.67\\plugin_info.ini",
        "plugin_info.ini",
    ),
    (
        "Users\\admin\\AppData\\Roaming\\Tencent\\xwechat\\ilink\\wechat\\cloud_account.txt",
        "cloud_account.txt",
    ),
    (
        "Users\\admin\\AppData\\Roaming\\Tencent\\xwechat\\login\\wxid_zuaa9igqlro22\\key_info.dat",
        "key_info.dat",
    ),
    (
        "Users\\admin\\AppData\\Roaming\\Tencent\\xwechat\\ilink\\kvcomm\\config.ini",
        "kvcomm_config.ini",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\contact\\contact.db",
        "contact.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\session\\session.db",
        "session.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\sns\\sns.db",
        "sns.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\favorite\\favorite.db",
        "favorite.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\message_0.db",
        "message_0.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\biz_message_0.db",
        "biz_message_0.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\message_fts.db",
        "message_fts.db",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\message_resource.db",
        "message_resource.db",
    ),
    // WAL companions for the merge path (same keys as their main db).
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\message_0.db-wal",
        "message_0.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\biz_message_0.db-wal",
        "biz_message_0.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\contact\\contact.db-wal",
        "contact.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\session\\session.db-wal",
        "session.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\sns\\sns.db-wal",
        "sns.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\favorite\\favorite.db-wal",
        "favorite.db-wal",
    ),
    (
        "Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\message\\message_fts.db-wal",
        "message_fts.db-wal",
    ),
];

fn main() {
    let mut args = std::env::args().skip(1);
    let image = args.next().expect("image path");
    let offset: u64 = args.next().expect("offset").parse().expect("offset number");
    let out_dir = args.next().expect("output dir");
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let reader = E01Reader::open(Path::new(&image)).expect("open e01");
    let fs = fs_ntfs::NtfsReader::open(Box::new(reader), offset).expect("open ntfs");

    for (source, name) in FILES {
        let mut file = match fs.open_file(source) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("SKIP {source}: {e}");
                continue;
            }
        };
        let out_path = Path::new(&out_dir).join(name);
        let mut out = std::fs::File::create(&out_path).expect("create output");
        let mut total = 0u64;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).expect("write");
            total += n as u64;
        }
        println!("OK {name} {total} bytes");
    }
}
