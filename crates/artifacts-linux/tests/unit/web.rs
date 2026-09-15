use super::*;

#[test]
fn parses_nginx_site_and_access_findings() {
    let config = r#"
    server {
      listen 80;
      server_name example.com www.example.com;
      root /var/www/html;
      access_log /var/log/nginx/access.log;
      error_log /var/log/nginx/error.log;
    }
    "#;
    let sites = parse_nginx_config(config).unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].server_kind, "nginx");
    assert!(sites[0].hostnames.contains(&"example.com".to_string()));
    assert!(sites[0]
        .document_roots
        .contains(&"/var/www/html".to_string()));

    let log = r#"192.0.2.10 - - [15/Jan/2024:10:30:45 +0000] "GET /products?id=1%20UNION%20SELECT%20password HTTP/1.1" 200 4532 "-" "sqlmap/1.7""#;
    let entries = parse_web_access_log(log).unwrap();
    assert_eq!(entries.len(), 1);
    let findings = detect_web_findings(&entries);
    assert!(findings
        .iter()
        .any(|finding| finding.finding_kind == "sqlInjection"));
    assert!(findings
        .iter()
        .any(|finding| finding.finding_kind == "scannerFingerprint"));
}

#[test]
fn parses_apache_virtual_host() {
    let config = r#"
    <VirtualHost *:8080>
      ServerName app.example.test
      ServerAlias www.example.test
      DocumentRoot "/srv/www/app"
      CustomLog /var/log/httpd/access_log combined
      ErrorLog /var/log/httpd/error_log
    </VirtualHost>
    "#;
    let sites = parse_apache_config(config).unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].server_kind, "apache");
    assert!(sites[0].listen.contains(&"*:8080".to_string()));
    assert!(sites[0].hostnames.contains(&"app.example.test".to_string()));
    assert!(sites[0]
        .document_roots
        .contains(&"/srv/www/app".to_string()));
}

#[test]
fn detects_web_shell_lines() {
    let findings = detect_web_shell("<?php echo shell_exec($_GET['cmd']);", 1);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].finding_kind, "webShellCandidate");
}

#[test]
fn vhost_prefixed_line_recovers_client_ip_and_annotates_vhost() {
    let log = "shop.example.com 198.51.100.23 - - [15/Jan/2024:10:30:45 +0000] \"GET /cart HTTP/1.1\" 200 512 \"-\" \"Mozilla/5.0\"\n";
    let (entries, stats) = parse_web_access_log_with_stats(log).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].client_ip, "198.51.100.23");
    assert_eq!(entries[0].vhost.as_deref(), Some("shop.example.com"));
    assert_eq!(stats.total_lines, 1);
    assert_eq!(stats.parsed_lines, 1);
    assert_eq!(stats.vhost_prefixed_lines, 1);
}

#[test]
fn hostname_client_keeps_first_token_without_vhost_annotation() {
    // Apache with HostnameLookups logs a resolved host name as the client;
    // the following `- %u` tokens are not IPs, so nothing is re-interpreted.
    let log = "proxy.example.com - - [15/Jan/2024:10:30:45 +0000] \"GET / HTTP/1.1\" 200 1\n";
    let entries = parse_web_access_log(log).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].client_ip, "proxy.example.com");
    assert_eq!(entries[0].vhost, None);
}

#[test]
fn access_log_stats_count_unparseable_lines() {
    let log = "192.0.2.10 - - [15/Jan/2024:10:30:45 +0000] \"GET /a HTTP/1.1\" 200 100\n\
               not a log line at all\n\
               \n\
               {\"json\":\"access\",\"line\":1}\n";
    let (entries, stats) = parse_web_access_log_with_stats(log).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(stats.total_lines, 3);
    assert_eq!(stats.parsed_lines, 1);
    assert_eq!(stats.unparsed_lines(), 2);
    assert_eq!(stats.vhost_prefixed_lines, 0);
}

#[test]
fn error_log_stats_count_non_blank_lines() {
    let log = "[Mon Jan 15 10:30:00.123456 2024] [core:error] something broke\n\n2024/01/15 10:30:01 [warn] 9#9: nginx warning\n";
    let (entries, stats) = parse_web_error_log_with_stats(log).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(stats.total_lines, 2);
    assert_eq!(stats.parsed_lines, 2);
    assert_eq!(stats.unparsed_lines(), 0);
}
