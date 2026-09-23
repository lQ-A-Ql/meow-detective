use crate::cluster_service::{
    parse_etcd_bolt_metadata, parse_etcd_wal, parse_kubeconfig, parse_kubernetes_artifact,
    parse_kubernetes_audit_log, parse_static_pod_manifests, KubernetesArtifactKind,
    KubernetesAuditIndicator, KubernetesParsedArtifact,
};
use crate::datasource_service::{
    detect_image_filesystem, expand_lvm_pool_candidates, ImageFilesystemKind, ImageFilesystemSource,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::PathBuf;

#[test]
fn kubeconfig_parser_redacts_credentials_but_keeps_routing_metadata() {
    let input = r#"apiVersion: v1
clusters:
- name: prod
  cluster:
    server: https://api.example.test:6443
    certificate-authority-data: SECRET-CA
contexts:
- name: prod-admin
  context:
    cluster: prod
    user: admin
    namespace: forensic
current-context: prod-admin
users:
- name: admin
  user:
    token: SUPER-SECRET
"#;

    let summary = parse_kubeconfig(input).expect("kubeconfig");

    assert_eq!(summary.current_context.as_deref(), Some("prod-admin"));
    assert_eq!(
        summary.clusters[0].server.as_deref(),
        Some("https://api.example.test:6443")
    );
    assert!(summary.clusters[0].certificate_authority_data_present);
    assert!(summary.users[0].token_present);
}

#[test]
fn static_manifest_parser_extracts_privilege_and_host_path_signals() {
    let input = r#"apiVersion: v1
kind: Pod
metadata:
  name: kube-apiserver
spec:
  hostNetwork: true
  containers:
  - name: kube-apiserver
    image: registry.k8s.io/kube-apiserver:v1.30.0
    command:
    - kube-apiserver
    securityContext:
      privileged: true
  volumes:
  - name: etc
    hostPath:
      path: /etc/kubernetes
"#;

    let summaries = parse_static_pod_manifests(input).expect("manifest");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name.as_deref(), Some("kube-apiserver"));
    assert!(summaries[0].host_network);
    assert_eq!(summaries[0].host_path_count, 1);
    assert_eq!(summaries[0].privileged_container_count, 1);
}

#[test]
fn audit_parser_flags_security_relevant_events_without_retaining_request_values() {
    let input = concat!(
        r#"{"requestReceivedTimestamp":"2026-09-20T00:00:00Z","verb":"get","user":{"username":"system:anonymous"},"objectRef":{"resource":"secrets","namespace":"default","name":"token"},"sourceIPs":["10.0.0.4"],"responseStatus":{"code":200}}"#,
        "\n",
        r#"{"verb":"create","user":{"username":"alice"},"objectRef":{"resource":"pods","subresource":"exec"},"requestObject":{"spec":{"containers":[{"securityContext":{"privileged":true}}]}}}"#,
    );

    let result = parse_kubernetes_audit_log(input).expect("audit");

    assert_eq!(result.events.len(), 2);
    assert!(result.events[0]
        .indicators
        .contains(&KubernetesAuditIndicator::SecretAccess));
    assert!(result.events[0]
        .indicators
        .contains(&KubernetesAuditIndicator::AnonymousAccess));
    assert!(result.events[1]
        .indicators
        .contains(&KubernetesAuditIndicator::InteractiveExec));
    assert!(result.events[1]
        .indicators
        .contains(&KubernetesAuditIndicator::PrivilegedPod));
}

#[test]
fn etcd_bolt_parser_reads_bucket_geometry_and_redacts_values() {
    let page_size = 4096usize;
    let mut image = vec![0u8; page_size * 3];
    for (offset, txid) in [(0usize, 1u64), (page_size, 2u64)] {
        image[offset..offset + 4].copy_from_slice(&0xED0C_DAEDu32.to_le_bytes());
        image[offset + 8..offset + 12].copy_from_slice(&(page_size as u32).to_le_bytes());
        image[offset + 16..offset + 24].copy_from_slice(&2u64.to_le_bytes());
        image[offset + 48..offset + 56].copy_from_slice(&txid.to_le_bytes());
    }
    let page = page_size * 2;
    image[page..page + 8].copy_from_slice(&2u64.to_le_bytes());
    image[page + 8..page + 10].copy_from_slice(&2u16.to_le_bytes());
    image[page + 10..page + 12].copy_from_slice(&1u16.to_le_bytes());
    image[page + 16..page + 20].copy_from_slice(&0u32.to_le_bytes());
    image[page + 20..page + 24].copy_from_slice(&64u32.to_le_bytes());
    image[page + 24..page + 28].copy_from_slice(&3u32.to_le_bytes());
    image[page + 28..page + 32].copy_from_slice(&2u32.to_le_bytes());
    image[page + 64..page + 67].copy_from_slice(b"foo");
    image[page + 67..page + 69].copy_from_slice(b"{}");

    let summary = parse_etcd_bolt_metadata(&image).expect("bolt");

    assert_eq!(summary.txid, 2);
    assert_eq!(summary.root_page, 2);
    assert_eq!(summary.key_count, 1);
    assert_eq!(summary.entries[0].key, "foo");
    assert!(summary.entries[0].value_redacted);
}

#[test]
fn etcd_wal_parser_validates_frame_and_protobuf_shape() {
    let payload = [0x08u8, 0x07u8];
    let crc = ceph_wire::crc32c::crc32c(0, &payload);
    let mut wal = Vec::new();
    wal.extend_from_slice(&crc.to_le_bytes());
    wal.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    wal.extend_from_slice(&[2, 0]);
    wal.extend_from_slice(&payload);
    wal.resize(16, 0);

    let summary = parse_etcd_wal(&wal).expect("wal");

    assert_eq!(summary.entry_count, 1);
    assert_eq!(summary.checksum_failures, 0);
    assert_eq!(summary.records[0].protobuf_field_count, 1);
}

#[test]
fn parsers_reject_unsafe_yaml_and_corrupt_bolt_input() {
    assert!(parse_kubeconfig("a: &anchor value\n").is_err());
    assert!(parse_etcd_bolt_metadata(&[0u8; 8192]).is_err());
}

#[test]
fn audit_parser_reports_bounded_invalid_lines_and_wal_tail() {
    let result = parse_kubernetes_audit_log("not-json\n{}\n").expect("audit diagnostics");
    assert_eq!(result.invalid_lines, 1);

    let result = parse_etcd_wal(&[1, 2, 3]).expect("truncated WAL tail is reportable");
    assert!(result.truncated_tail);
    assert!(result.records.is_empty());
}

#[test]
fn dispatch_api_routes_supported_content_parsers_and_skips_metadata_only_kinds() {
    let parsed = parse_kubernetes_artifact(
        KubernetesArtifactKind::Kubeconfig,
        b"apiVersion: v1\ncurrent-context: prod\n",
    )
    .expect("dispatch")
    .expect("kubeconfig result");
    assert!(matches!(parsed, KubernetesParsedArtifact::Kubeconfig(_)));
    assert_eq!(
        parse_kubernetes_artifact(KubernetesArtifactKind::Certificate, b"pem")
            .expect("metadata dispatch"),
        None
    );
}

#[test]
#[ignore = "requires FORENSICS_K8S_CLUSTER_ROOT real Kubernetes E01 cluster sample"]
fn real_kubernetes_sample_reads_and_parses_discovered_control_plane_artifacts() {
    let root = std::env::var_os("FORENSICS_K8S_CLUSTER_ROOT")
        .map(PathBuf::from)
        .expect("set FORENSICS_K8S_CLUSTER_ROOT");
    let mut images = std::fs::read_dir(&root)
        .expect("sample root")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("E01"))
        })
        .collect::<Vec<_>>();
    images.sort();
    assert!(!images.is_empty(), "sample root must contain E01 members");

    let target_paths = [
        (
            "etc/kubernetes/admin.conf",
            KubernetesArtifactKind::Kubeconfig,
        ),
        (
            "etc/kubernetes/manifests/kube-apiserver.yaml",
            KubernetesArtifactKind::StaticPodManifest,
        ),
        (
            "var/log/kubernetes/audit.log",
            KubernetesArtifactKind::AuditLog,
        ),
        (
            "var/lib/etcd/member/snap/db",
            KubernetesArtifactKind::EtcdBackend,
        ),
    ];
    let mut observed = 0usize;
    let mut parsed = 0usize;
    let mut failures = Vec::new();
    for image in images {
        let mut reader = E01Reader::open(&image).expect("open E01");
        let mut probe = detect_image_filesystem(&mut reader).expect("probe E01");
        expand_lvm_pool_candidates(&mut probe, &image, &domain::DataSourceKind::E01);
        for candidate in probe.candidates.iter().filter(|candidate| {
            matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume)
                && matches!(
                    candidate.kind,
                    ImageFilesystemKind::Ext4
                        | ImageFilesystemKind::Xfs
                        | ImageFilesystemKind::Btrfs
                )
        }) {
            let Some(identity) = candidate.lvm_identity.as_ref() else {
                continue;
            };
            let reader: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&image).expect("reopen E01"));
            let pool = fs_lvm::LvmPool::discover(vec![reader], identity.pv_offsets.clone())
                .expect("discover LVM pool");
            let Some(lv_index) = pool
                .list_volumes()
                .iter()
                .position(|volume| volume.name == identity.lv_name)
            else {
                continue;
            };
            let lv_reader = pool.open_volume(lv_index).expect("open LV");
            let fs: Box<dyn FileSystemReader> = match candidate.kind {
                ImageFilesystemKind::Ext4 => Box::new(
                    fs_ext4::Ext4Reader::open(Box::new(lv_reader), 0).expect("open ext4 LV"),
                ),
                ImageFilesystemKind::Xfs => {
                    Box::new(fs_xfs::XfsReader::open(Box::new(lv_reader), 0).expect("open xfs LV"))
                }
                ImageFilesystemKind::Btrfs => Box::new(
                    fs_btrfs::BtrfsReader::open(Box::new(lv_reader), 0).expect("open btrfs LV"),
                ),
                _ => continue,
            };
            for (path, kind) in target_paths {
                let Ok(mut file) = fs.open_file(path) else {
                    continue;
                };
                observed += 1;
                let mut bytes = Vec::new();
                file.by_ref()
                    .take(8 * 1024 * 1024)
                    .read_to_end(&mut bytes)
                    .expect("read Kubernetes artifact");
                match crate::cluster_service::parse_kubernetes_artifact(kind, &bytes) {
                    Ok(Some(_)) => parsed += 1,
                    Ok(None) => failures.push(format!("{path}: parser skipped")),
                    Err(error) => failures.push(format!("{path}: {error}")),
                }
                eprintln!(
                    "Kubernetes artifact: image={} lv={} path={} bytes={} kind={kind:?}",
                    image.display(),
                    identity.lv_name,
                    path,
                    bytes.len()
                );
            }
        }
    }
    eprintln!(
        "Kubernetes sample content verification: observed={observed} parsed={parsed} failures={failures:?}"
    );
    assert!(
        observed >= 2,
        "sample should expose control-plane artifacts"
    );
    assert!(
        parsed >= 2,
        "sample should parse kubeconfig and static Pod manifest"
    );
}

#[test]
#[ignore = "requires FORENSICS_K8S_CLUSTER_ROOT real Kubernetes E01 cluster sample"]
fn real_kubernetes_sample_host_identity_and_cni_paths_are_accounted_for() {
    let root = std::env::var_os("FORENSICS_K8S_CLUSTER_ROOT")
        .map(PathBuf::from)
        .expect("set FORENSICS_K8S_CLUSTER_ROOT");
    let mut images = std::fs::read_dir(root)
        .expect("sample root")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("E01"))
        })
        .collect::<Vec<_>>();
    images.sort();
    assert_eq!(
        images.len(),
        4,
        "Kubernetes sample has four evidence members"
    );
    for (index, image) in images.into_iter().enumerate() {
        let mut reader = E01Reader::open(&image).expect("open E01");
        let mut probe = detect_image_filesystem(&mut reader).expect("probe E01");
        expand_lvm_pool_candidates(&mut probe, &image, &domain::DataSourceKind::E01);
        let mut inspected_roots = 0;
        for candidate in probe.candidates.iter().filter(|candidate| {
            matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume)
                && matches!(
                    candidate.kind,
                    ImageFilesystemKind::Ext4
                        | ImageFilesystemKind::Xfs
                        | ImageFilesystemKind::Btrfs
                )
        }) {
            let Some(identity) = candidate.lvm_identity.as_ref() else {
                continue;
            };
            let reader: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&image).expect("reopen E01"));
            let pool = fs_lvm::LvmPool::discover(vec![reader], identity.pv_offsets.clone())
                .expect("LVM pool");
            let Some(lv_index) = pool
                .list_volumes()
                .iter()
                .position(|volume| volume.name == identity.lv_name)
            else {
                continue;
            };
            let lv_reader = pool.open_volume(lv_index).expect("open LV");
            let fs: Box<dyn FileSystemReader> = match candidate.kind {
                ImageFilesystemKind::Ext4 => {
                    Box::new(fs_ext4::Ext4Reader::open(Box::new(lv_reader), 0).expect("ext4"))
                }
                ImageFilesystemKind::Xfs => {
                    Box::new(fs_xfs::XfsReader::open(Box::new(lv_reader), 0).expect("xfs"))
                }
                ImageFilesystemKind::Btrfs => {
                    Box::new(fs_btrfs::BtrfsReader::open(Box::new(lv_reader), 0).expect("btrfs"))
                }
                _ => continue,
            };
            inspected_roots += 1;
            let hostname = fs
                .read_file_range("etc/hostname", 0, 128)
                .expect("hostname");
            let expected = ["master", "node1", "node2", "localhost.localdomain"][index];
            assert_eq!(String::from_utf8_lossy(&hostname).trim(), expected);
            let link = fs
                .read_file_range("etc/os-release", 0, 128)
                .expect("os-release link");
            assert_eq!(
                String::from_utf8_lossy(&link).trim(),
                "../usr/lib/os-release"
            );
            let release = fs
                .read_file_range("usr/lib/os-release", 0, 512)
                .expect("os-release content");
            assert!(String::from_utf8_lossy(&release).contains("VERSION_ID=\"7\""));
            let cni = fs.list_children("etc/cni/net.d");
            if index < 3 {
                assert!(cni
                    .expect("CNI directory")
                    .iter()
                    .any(|entry| entry.name == "10-calico.conflist"));
                let config = fs
                    .read_file_range("etc/cni/net.d/10-calico.conflist", 0, 16 * 1024)
                    .expect("CNI config");
                let summary = crate::cluster_service::parse_cni_config(&config)
                    .expect("parse real CNI config");
                assert!(summary.plugin_types.iter().any(|plugin| plugin == "calico"));
            } else {
                assert!(cni.is_err(), "fourth member has no CNI directory");
            }
            let manifests = fs.list_children("etc/kubernetes/manifests");
            if index == 0 {
                assert!(manifests
                    .expect("control-plane manifests")
                    .iter()
                    .any(|entry| entry.name == "kube-apiserver.yaml"));
                let manifest = fs
                    .read_file_range("etc/kubernetes/manifests/kube-apiserver.yaml", 0, 16 * 1024)
                    .expect("API server manifest");
                let parsed = parse_static_pod_manifests(
                    std::str::from_utf8(&manifest).expect("UTF-8 manifest"),
                )
                .expect("parse API server manifest");
                assert!(parsed
                    .iter()
                    .flat_map(|pod| &pod.containers)
                    .any(|container| {
                        container
                            .image
                            .as_deref()
                            .is_some_and(|image| image.contains("kube-apiserver:"))
                    }));
            } else if index < 3 {
                assert!(manifests.expect("worker manifest directory").is_empty());
            } else {
                assert!(manifests.is_err(), "fourth member has no static manifests");
            }
        }
        assert!(
            inspected_roots > 0,
            "each Kubernetes member must expose a root LV"
        );
    }
}
