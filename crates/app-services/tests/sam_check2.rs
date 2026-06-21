use evidence_core::EvidenceReader;
use evidence_core::FileSystemReader;
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use std::io::Read;

fn sample_path() -> std::path::PathBuf {
    std::env::var("FORENSICS_LIUYANG_E01_FIXTURE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_LIUYANG_E01_FIXTURE to the liuyang E01 sample path")
        })
}

// Local run:
//   $env:FORENSICS_LIUYANG_E01_FIXTURE='<path-to-liuyang-sample.E01>'
//   cargo test -p app-services --test sam_check2 -- --nocapture
#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn sam_extraction_from_liuyang_e01() {
    let fixture_path = sample_path();
    eprintln!("Opening E01: {}", fixture_path.display());

    // Step 1: Open E01 and probe filesystems
    let mut reader = E01Reader::open(&fixture_path).unwrap();

    // Detect filesystem candidates
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert!(
        !probe.candidates.is_empty(),
        "E01 sample should expose at least one filesystem candidate"
    );

    eprintln!(
        "Probe: {} partitions, {} candidates",
        probe.partitions.len(),
        probe.candidates.len()
    );

    let ntfs_candidates: Vec<_> = probe
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .collect();

    assert!(
        !ntfs_candidates.is_empty(),
        "E01 sample should have at least one NTFS candidate"
    );

    // Step 2: Find the NTFS partition that contains Windows/System32/config/SAM
    let sam_path = "Windows/System32/config/SAM";
    let mut sam_bytes = None;
    let mut chosen_offset = 0u64;

    for ntfs in &ntfs_candidates {
        eprintln!("Trying NTFS at offset: {}", ntfs.offset);
        let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path).unwrap());
        let fs = match NtfsReader::open(boxed, ntfs.offset) {
            Ok(fs) => fs,
            Err(e) => {
                eprintln!("  Failed to open NTFS at offset {}: {e}", ntfs.offset);
                continue;
            }
        };
        match fs.open_file(sam_path) {
            Ok(mut reader) => {
                let mut bytes = Vec::new();
                if reader.read_to_end(&mut bytes).is_ok() && bytes.starts_with(b"regf") {
                    eprintln!(
                        "  Found SAM hive at offset {}: {} bytes",
                        ntfs.offset,
                        bytes.len()
                    );
                    sam_bytes = Some(bytes);
                    chosen_offset = ntfs.offset;
                    break;
                }
                eprintln!(
                    "  SAM file at offset {} is not a valid registry hive",
                    ntfs.offset
                );
            }
            Err(e) => {
                eprintln!("  SAM not found at offset {}: {e}", ntfs.offset);
            }
        }
    }

    let sam_bytes = sam_bytes.unwrap_or_else(|| {
        panic!(
            "SAM hive not found on any of {} NTFS candidates",
            ntfs_candidates.len()
        )
    });
    eprintln!("Using SAM from NTFS at offset: {}", chosen_offset);

    // Step 3: Extract SAM fields
    let info = artifacts_windows::extract_sam_fields(&sam_bytes, sam_path).unwrap();

    // Print results
    eprintln!(
        "SAM extraction: {} users, {} groups, {} warnings",
        info.users.len(),
        info.groups.len(),
        info.warnings.len()
    );

    if !info.warnings.is_empty() {
        for w in &info.warnings {
            eprintln!("  WARNING: {w}");
        }
    }

    let mut sorted_users = info.users.clone();
    sorted_users.sort_by_key(|u| u.rid);

    eprintln!("\nUsers extracted from SAM:");
    for user in &sorted_users {
        eprintln!(
            "  RID={:<6} username={:<25} full_name={:<30} comment={:<20} admin_count={} disabled={} locked={}",
            user.rid,
            user.username,
            if user.full_name.is_empty() { "-" } else { &user.full_name },
            if user.comment.is_empty() { "-" } else { &user.comment },
            user.admin_count,
            user.account_disabled,
            user.account_locked,
        );
        if !user.group_memberships.is_empty() {
            eprintln!("    groups: {:?}", user.group_memberships);
        }
        if let Some(login) = user.last_login {
            eprintln!("    last_login: {}", login);
        }
        if let Some(pwd_set) = user.password_last_set {
            eprintln!("    password_last_set: {}", pwd_set);
        }
    }

    if !info.groups.is_empty() {
        eprintln!("\nGroups extracted from SAM:");
        let mut sorted_groups = info.groups.clone();
        sorted_groups.sort_by_key(|g| g.rid);
        for group in &sorted_groups {
            eprintln!(
                "  RID={:<6} name={:<30} members={:?}",
                group.rid, group.name, group.members
            );
        }
    }

    if let Some(ref policy) = info.password_policy {
        eprintln!(
            "\nPassword policy: max_age={}d min_age={}d min_len={} hist={} threshold={} lockout={}m obs={}m",
            policy.max_password_age_days,
            policy.min_password_age_days,
            policy.min_password_length,
            policy.password_history_length,
            policy.lockout_threshold,
            policy.lockout_duration_minutes,
            policy.lockout_observation_window_minutes,
        );
    }

    // Step 4: Assertions
    // At least 4 default users (Administrator, Guest, DefaultAccount, WDAGUtilityAccount)
    assert!(
        info.users.len() >= 4,
        "Expected at least 4 users in SAM, got {} users",
        info.users.len()
    );

    // Verify Administrator (RID 500)
    let admin = info
        .users
        .iter()
        .find(|u| u.username == "Administrator")
        .unwrap_or_else(|| {
            panic!(
                "Administrator not found in SAM users: {:?}",
                info.users.iter().map(|u| &u.username).collect::<Vec<_>>()
            )
        });
    assert_eq!(
        admin.rid, 500,
        "Administrator should have RID 500, got RID {}",
        admin.rid
    );

    // Verify Guest (RID 501)
    let guest = info
        .users
        .iter()
        .find(|u| u.username == "Guest")
        .unwrap_or_else(|| {
            panic!(
                "Guest not found in SAM users: {:?}",
                info.users.iter().map(|u| &u.username).collect::<Vec<_>>()
            )
        });
    assert_eq!(
        guest.rid, 501,
        "Guest should have RID 501, got RID {}",
        guest.rid
    );

    // Verify at least some groups exist
    assert!(
        !info.groups.is_empty(),
        "Expected at least some groups in SAM"
    );

    // Check for expected built-in groups
    let expected_groups = ["Administrators", "Users"];
    for expected in &expected_groups {
        let found = info.groups.iter().any(|g| g.name == *expected);
        assert!(
            found,
            "Expected group '{}' not found in SAM groups: {:?}",
            expected,
            info.groups.iter().map(|g| &g.name).collect::<Vec<_>>()
        );
    }

    eprintln!("\n=== SAM extraction verification PASSED ===");
}
