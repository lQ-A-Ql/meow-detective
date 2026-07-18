use persistence_sqlite::{
    open_in_memory,
    repositories::filesystem_locator_repo::{
        FilesystemDirectoryLocatorRecord, FilesystemFileLocatorRecord, FilesystemLocatorRepo,
    },
    runner,
};

const SCOPE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SCOPE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn source_connection() -> rusqlite::Connection {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    connection
}

#[test]
fn directory_locators_round_trip_and_replace_atomically() {
    let connection = source_connection();
    let repo = FilesystemLocatorRepo::new(&connection);
    let first = vec![
        FilesystemDirectoryLocatorRecord {
            path: "etc".to_string(),
            locator: "128".to_string(),
        },
        FilesystemDirectoryLocatorRecord {
            path: "var/www".to_string(),
            locator: "256".to_string(),
        },
    ];
    repo.replace_directory_locators("source-1", 2, "xfs", SCOPE_A, &first)
        .expect("persist directory locators");
    assert_eq!(
        repo.list_directory_locators("source-1", 2, "XFS", SCOPE_A)
            .expect("load directory locators"),
        first
    );

    let replacement = vec![FilesystemDirectoryLocatorRecord {
        path: "home".to_string(),
        locator: "512".to_string(),
    }];
    repo.replace_directory_locators("source-1", 2, "xfs", SCOPE_A, &replacement)
        .expect("replace directory locators");
    assert_eq!(
        repo.list_directory_locators("source-1", 2, "xfs", SCOPE_A)
            .expect("load replacement"),
        replacement
    );
    assert!(repo
        .list_directory_locators("source-1", 2, "xfs", SCOPE_B)
        .expect("load independent locator scope")
        .is_empty());
}

#[test]
fn directory_locators_reject_unsorted_or_ambiguous_records() {
    let connection = source_connection();
    let repo = FilesystemLocatorRepo::new(&connection);
    let unsorted = vec![
        FilesystemDirectoryLocatorRecord {
            path: "var".to_string(),
            locator: "2".to_string(),
        },
        FilesystemDirectoryLocatorRecord {
            path: "etc".to_string(),
            locator: "1".to_string(),
        },
    ];
    assert!(repo
        .replace_directory_locators("source-1", 0, "xfs", SCOPE_A, &unsorted)
        .is_err());
    assert!(repo
        .replace_directory_locators(
            "source:1",
            0,
            "xfs",
            SCOPE_A,
            &[FilesystemDirectoryLocatorRecord {
                path: "etc".to_string(),
                locator: "1".to_string(),
            }],
        )
        .is_err());
}

#[test]
fn file_locators_round_trip_independently_from_directory_locators() {
    let connection = source_connection();
    let repo = FilesystemLocatorRepo::new(&connection);
    let files = vec![
        FilesystemFileLocatorRecord {
            path: "etc/hosts".to_string(),
            locator: "129".to_string(),
        },
        FilesystemFileLocatorRecord {
            path: "var/www/index.html".to_string(),
            locator: "257".to_string(),
        },
    ];
    repo.replace_file_locators("source-1", 2, "xfs", SCOPE_A, &files)
        .expect("persist file locators");

    assert_eq!(
        repo.list_file_locators("source-1", 2, "XFS", SCOPE_A)
            .expect("load file locators"),
        files
    );
    assert!(repo
        .list_directory_locators("source-1", 2, "xfs", SCOPE_A)
        .expect("load directory locators")
        .is_empty());
}

#[test]
fn locator_scope_must_be_a_canonical_sha256_hex_digest() {
    let connection = source_connection();
    let repo = FilesystemLocatorRepo::new(&connection);
    let record = [FilesystemFileLocatorRecord {
        path: "etc/hosts".to_string(),
        locator: "129".to_string(),
    }];

    assert!(repo
        .replace_file_locators("source-1", 2, "xfs", "not-a-digest", &record)
        .is_err());
    assert!(repo
        .replace_file_locators(
            "source-1",
            2,
            "xfs",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            &record,
        )
        .is_err());
}
