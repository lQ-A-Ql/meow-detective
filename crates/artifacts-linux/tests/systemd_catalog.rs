use artifacts_linux::{parse_systemd_unit, SystemdUnitCatalog, SystemdUnitFile};
use std::collections::BTreeMap;
type Files = BTreeMap<String, (bool, Vec<u8>)>;
fn fixture(entries: &[(&str, bool, &str)]) -> (SystemdUnitCatalog, Files) {
    let files = entries
        .iter()
        .map(|(path, link, text)| ((*path).into(), (*link, text.as_bytes().to_vec())))
        .collect::<Files>();
    let catalog =
        SystemdUnitCatalog::new(files.iter().map(|(path, (is_link, _))| SystemdUnitFile {
            path: path.clone(),
            is_symlink: *is_link,
        }))
        .unwrap();
    (catalog, files)
}
fn read(
    files: &Files,
    path: &str,
    limit: usize,
) -> Result<Vec<u8>, artifacts_linux::LinuxArtifactError> {
    Ok(files
        .get(path)
        .unwrap_or_else(|| panic!("read outside captured catalog: {path}"))
        .1
        .iter()
        .copied()
        .take(limit)
        .collect())
}
#[test]
fn symlink_enablement_merges_dropins_by_name_and_resets_previous_commands() {
    let (catalog,files)=fixture(&[
        ("/usr/lib/systemd/system/worker.service",false,"[Unit]\nDescription=Worker\n[Service]\nExecStart=/old\nRestart=no\n[Install]\nWantedBy=multi-user.target\n"),
        ("/etc/systemd/system/multi-user.target.wants/worker.service",true,"/usr/lib/systemd/system/worker.service"),
        ("/usr/lib/systemd/system/worker.service.d/20-command.conf",false,"[Service]\nExecStart=/vendor\n"),
        ("/etc/systemd/system/worker.service.d/20-command.conf",false,"[Service]\nExecStart=\nExecStart=/new \\\n# ignored comment\n --flag\nUser=daemon\nRestart=always\n"),
        ("/etc/systemd/system/service.d/10-common.conf",false,"[Service]\nWorkingDirectory=/work\n"),
    ]);
    let unit = catalog
        .resolve("worker.service", |path, limit| read(&files, path, limit))
        .unwrap();
    assert_eq!(unit.state, "enabled");
    assert_eq!(unit.exec_start, ["/new  --flag"]);
    assert_eq!(unit.user.as_deref(), Some("daemon"));
    assert_eq!(unit.restart.as_deref(), Some("always"));
    assert_eq!(unit.working_directory.as_deref(), Some("/work"));
    assert_eq!(unit.drop_ins.len(), 2);
    assert!(!unit
        .source_paths
        .iter()
        .any(|path| path.starts_with("/usr/lib/systemd/system/worker.service.d")));
}
#[test]
fn install_metadata_or_a_plain_file_in_wants_does_not_prove_enablement() {
    let (catalog, files) = fixture(&[
        (
            "/usr/lib/systemd/system/demo.service",
            false,
            "[Service]\nExecStart=/bin/demo\n[Install]\nWantedBy=multi-user.target\n",
        ),
        (
            "/etc/systemd/system/multi-user.target.wants/demo.service",
            false,
            "[Service]\nExecStart=/bin/demo\n",
        ),
    ]);
    let unit = catalog
        .resolve("demo.service", |path, limit| read(&files, path, limit))
        .unwrap();
    assert_eq!(unit.state, "disabled");
    assert!(!unit.enabled);
    assert_eq!(
        parse_systemd_unit(
            "static.service",
            b"[Service]\nExecStart=/bin/demo\n[Install]\n"
        )
        .unwrap()
        .state,
        "static"
    );
}
#[test]
fn a_higher_priority_dev_null_mask_blocks_vendor_definition_without_host_access() {
    let (catalog, files) = fixture(&[
        ("/etc/systemd/system/demo.service", true, "/dev/null"),
        (
            "/usr/lib/systemd/system/demo.service",
            false,
            "[Service]\nExecStart=/bin/demo\n",
        ),
    ]);
    let unit = catalog
        .resolve("demo.service", |path, limit| read(&files, path, limit))
        .unwrap();
    assert_eq!(unit.state, "masked");
    assert!(unit.exec_start.is_empty());
    assert_eq!(unit.source_paths, ["/etc/systemd/system/demo.service"]);
}
#[test]
fn dangling_symlinks_and_cycles_are_explicit_failures() {
    let (catalog, files) = fixture(&[
        (
            "/etc/systemd/system/broken.service",
            true,
            "missing.service",
        ),
        ("/etc/systemd/system/one.service", true, "two.service"),
        ("/etc/systemd/system/two.service", true, "one.service"),
    ]);
    assert!(catalog
        .resolve("broken.service", |path, limit| read(&files, path, limit))
        .is_err());
    assert!(catalog
        .resolve("one.service", |path, limit| read(&files, path, limit))
        .is_err());
}
#[test]
fn template_instances_use_their_own_enablement_and_override() {
    let (catalog, files) = fixture(&[
        (
            "/usr/lib/systemd/system/worker@.service",
            false,
            "[Service]\nExecStart=/bin/worker %i\n[Install]\nWantedBy=multi-user.target\n",
        ),
        (
            "/etc/systemd/system/multi-user.target.wants/worker@a.service",
            true,
            "/usr/lib/systemd/system/worker@.service",
        ),
        (
            "/etc/systemd/system/worker@a.service.d/20-user.conf",
            false,
            "[Service]\nUser=first\n",
        ),
    ]);
    let links = catalog.prepare_links(|path, limit| read(&files, path, limit));
    let first = catalog
        .resolve_with_links("worker@a.service", &links, |path, limit| {
            read(&files, path, limit)
        })
        .unwrap();
    assert_eq!(first.state, "enabled");
    assert_eq!(first.user.as_deref(), Some("first"));
    let second = catalog
        .resolve_with_links("worker@b.service", &links, |path, limit| {
            read(&files, path, limit)
        })
        .unwrap();
    assert_eq!(second.state, "disabled");
    assert!(second.user.is_none());
}
