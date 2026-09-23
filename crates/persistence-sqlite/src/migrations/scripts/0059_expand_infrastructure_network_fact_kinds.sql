ALTER TABLE infrastructure_network_facts RENAME TO infrastructure_network_facts_old;

CREATE TABLE infrastructure_network_facts (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    environment_object_id TEXT NOT NULL REFERENCES environment_objects(id) ON DELETE CASCADE,
    file_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    line_number INTEGER NOT NULL CHECK (line_number > 0),
    fact_kind TEXT NOT NULL CHECK (fact_kind IN (
        'hostname_mapping', 'dns_server', 'interface_address',
        'gateway', 'bridge_member', 'cluster_link', 'cni_network',
        'platform_version', 'kubernetes_version', 'kubernetes_node',
        'kubernetes_service', 'kubernetes_endpoint_slice',
        'kubernetes_ingress', 'kubernetes_network_policy'
    )),
    subject TEXT NOT NULL,
    value TEXT NOT NULL,
    assertion_kind TEXT NOT NULL DEFAULT 'configured' CHECK (assertion_kind IN (
        'configured', 'observed'
    )),
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN (
        'candidate', 'corroborated', 'proven', 'conflicted'
    )),
    parser TEXT NOT NULL,
    source_artifact_id TEXT NOT NULL,
    CHECK (length(subject) <= 256 AND length(value) <= 1024),
    UNIQUE (case_id, data_source_id, file_id, line_number, fact_kind, subject, value)
);

INSERT INTO infrastructure_network_facts (
    id, case_id, data_source_id, environment_object_id, file_id,
    source_path, line_number, fact_kind, subject, value, assertion_kind,
    confidence, parser, source_artifact_id
)
SELECT id, case_id, data_source_id, environment_object_id, file_id,
       source_path, line_number, fact_kind, subject, value, assertion_kind,
       confidence, parser, source_artifact_id
FROM infrastructure_network_facts_old;

DROP TABLE infrastructure_network_facts_old;

CREATE INDEX idx_infrastructure_network_facts_case_kind
ON infrastructure_network_facts(case_id, fact_kind, environment_object_id);

CREATE INDEX idx_infrastructure_network_facts_source
ON infrastructure_network_facts(data_source_id, source_path, line_number);
