use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_test_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZASSET (
                    Z_PK INTEGER PRIMARY KEY,
                    ZFILENAME TEXT,
                    ZWIDTH REAL,
                    ZHEIGHT REAL,
                    ZDATECREATED REAL
                );
                CREATE TABLE ZGENERICALBUM (
                    Z_PK INTEGER PRIMARY KEY,
                    ZTITLE TEXT
                );
                CREATE TABLE ZASSETALBUMS (
                    asset_pk INTEGER,
                    album_pk INTEGER
                );

                INSERT INTO ZASSET VALUES (1, 'IMG_0001.JPG', 4032.0, 3024.0, 689500800.0);
                INSERT INTO ZASSET VALUES (2, 'IMG_0002.PNG', 1920.0, 1080.0, 689860800.0);
                INSERT INTO ZASSET VALUES (3, 'IMG_0003.HEIC', 3024.0, 4032.0, NULL);

                INSERT INTO ZGENERICALBUM VALUES (10, 'Favorites');
                INSERT INTO ZGENERICALBUM VALUES (11, 'Vacation 2024');

                INSERT INTO ZASSETALBUMS VALUES (1, 10);
                INSERT INTO ZASSETALBUMS VALUES (1, 11);
                INSERT INTO ZASSETALBUMS VALUES (2, 11);",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

fn make_empty_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZASSET (
                    Z_PK INTEGER PRIMARY KEY,
                    ZFILENAME TEXT,
                    ZWIDTH REAL,
                    ZHEIGHT REAL,
                    ZDATECREATED REAL
                );
                CREATE TABLE ZGENERICALBUM (
                    Z_PK INTEGER PRIMARY KEY,
                    ZTITLE TEXT
                );",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

#[test]
fn parse_photos_basic() {
    let db = make_test_db();
    let photos = parse_photos(&db).expect("parse photos");
    assert_eq!(photos.len(), 3);

    // IMG_0001.JPG: in Favorites + Vacation 2024
    assert_eq!(photos[0].filename.as_deref(), Some("IMG_0001.JPG"));
    assert_eq!(photos[0].width, Some(4032.0));
    assert_eq!(photos[0].height, Some(3024.0));
    assert!(photos[0].created_at.is_some());
    assert_eq!(photos[0].albums.len(), 2);
    assert!(photos[0].albums.contains(&"Favorites".to_string()));
    assert!(photos[0].albums.contains(&"Vacation 2024".to_string()));

    // IMG_0002.PNG: in Vacation 2024 only
    assert_eq!(photos[1].filename.as_deref(), Some("IMG_0002.PNG"));
    assert_eq!(photos[1].albums.len(), 1);
    assert_eq!(photos[1].albums[0], "Vacation 2024");

    // IMG_0003.HEIC: no creation date, no albums
    assert_eq!(photos[2].filename.as_deref(), Some("IMG_0003.HEIC"));
    assert!(photos[2].created_at.is_none());
    assert!(photos[2].albums.is_empty());
}

#[test]
fn parse_photos_empty_db() {
    let db = make_empty_db();
    let photos = parse_photos(&db).expect("parse");
    assert!(photos.is_empty());
}

#[test]
fn parse_photos_not_a_db() {
    let result = parse_photos(b"invalid data");
    assert!(result.is_err());
}

#[test]
fn parse_photos_no_albums_table() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZASSET (
                    Z_PK INTEGER PRIMARY KEY,
                    ZFILENAME TEXT,
                    ZWIDTH REAL,
                    ZHEIGHT REAL,
                    ZDATECREATED REAL
                );
                INSERT INTO ZASSET VALUES (1, 'photo.jpg', 2048.0, 1536.0, 689500800.0);",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let photos = parse_photos(&buf).expect("parse");
    assert_eq!(photos.len(), 1);
    assert!(photos[0].albums.is_empty());
}
