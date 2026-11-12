use crate::cluster_service::{parse_kubernetes_network_resources, KubernetesNetworkResource};

#[test]
fn network_resource_parser_extracts_nodes_services_endpoints_and_policies() {
    let resources = parse_kubernetes_network_resources(
        r#"
---
apiVersion: v1
kind: Node
metadata:
  name: node-a
status:
  podCIDR: 10.244.1.0/24
  addresses:
  - type: InternalIP
    address: 192.0.2.10
---
apiVersion: v1
kind: Service
metadata:
  name: api
  namespace: default
spec:
  type: ClusterIP
  clusterIP: 10.96.0.10
  selector:
    app: api
  ports:
  - port: 443
    protocol: TCP
---
apiVersion: discovery.k8s.io/v1
kind: EndpointSlice
metadata:
  name: api-1
  namespace: default
  labels:
    kubernetes.io/service-name: api
addressType: IPv4
ports:
- name: https
  protocol: TCP
  port: 8443
endpoints:
- addresses:
  - 10.244.1.4
  nodeName: node-a
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: api-policy
  namespace: default
spec:
  podSelector:
    matchLabels:
      app: api
  policyTypes:
  - Ingress
"#,
    )
    .unwrap();
    assert_eq!(resources.len(), 4);
    assert!(
        matches!(&resources[0], KubernetesNetworkResource::Node(node) if node.addresses[0].1 == "192.0.2.10")
    );
    assert!(
        matches!(&resources[1], KubernetesNetworkResource::Service(service) if service.cluster_ip.as_deref() == Some("10.96.0.10"))
    );
    assert!(
        matches!(&resources[2], KubernetesNetworkResource::EndpointSlice(slice) if slice.addresses == vec!["10.244.1.4"])
    );
    assert!(
        matches!(&resources[3], KubernetesNetworkResource::NetworkPolicy(policy) if policy.policy_types == vec!["Ingress"])
    );
}
