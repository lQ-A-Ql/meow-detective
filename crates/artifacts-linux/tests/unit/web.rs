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
