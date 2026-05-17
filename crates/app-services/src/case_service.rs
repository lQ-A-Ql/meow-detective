use transport::dto::{CaseMetricsDto, CaseSummaryDto, RecentObjectDto};

pub fn get_current_case() -> CaseSummaryDto {
    CaseSummaryDto {
        id: "case-2025-001".into(),
        name: "Windows 11 工作站镜像".into(),
        number: Some("LAB-2025-001".into()),
        examiner: Some("取证分析员 A".into()),
        created_at: "2025-02-14T09:30:00Z".into(),
        updated_at: "2025-02-16T18:42:00Z".into(),
    }
}

pub fn get_case_metrics() -> CaseMetricsDto {
    CaseMetricsDto {
        data_source_count: 3,
        indexed_file_count: 128_440,
        timeline_event_count: 42_118,
        artifact_count: 3_284,
    }
}

pub fn get_recent_objects() -> Vec<RecentObjectDto> {
    vec![
        RecentObjectDto {
            id: "file-001".into(),
            title: "Downloads/AnyDesk.exe".into(),
            detail: "可执行文件，命中近期访问".into(),
            time: "2025-02-16T16:02:12Z".into(),
            kind: "file".into(),
        },
        RecentObjectDto {
            id: "reg-001".into(),
            title: "RunMRU".into(),
            detail: "最近运行项包含 powershell".into(),
            time: "2025-02-16T15:48:09Z".into(),
            kind: "registry".into(),
        },
        RecentObjectDto {
            id: "net-001".into(),
            title: "10.10.20.15:443".into(),
            detail: "可疑外联目的地址".into(),
            time: "2025-02-16T14:13:55Z".into(),
            kind: "network".into(),
        },
    ]
}
