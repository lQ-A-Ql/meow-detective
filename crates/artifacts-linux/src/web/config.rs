use super::util::{
    brace_delta, push_first_token, push_tokens, strip_inline_comment, virtual_host_listen,
};
use super::WebSite;

pub fn parse_nginx_config(content: &str) -> Result<Vec<WebSite>, crate::LinuxArtifactError> {
    let mut sites = Vec::new();
    let mut current = None;
    let mut depth = 0i32;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = strip_inline_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if current.is_none() && trimmed.starts_with("server") && trimmed.contains('{') {
            current = Some(new_site(
                "nginx",
                format!("nginx server line {line_number}"),
                line_number,
            ));
            depth = brace_delta(trimmed);
            finish_closed_site(&mut current, &mut sites, depth);
            continue;
        }
        if let Some(site) = current.as_mut() {
            collect_nginx_directive(site, trimmed);
            depth += brace_delta(trimmed);
            finish_closed_site(&mut current, &mut sites, depth);
        }
    }
    if let Some(site) = current {
        sites.push(site);
    }
    Ok(sites)
}

pub fn parse_apache_config(content: &str) -> Result<Vec<WebSite>, crate::LinuxArtifactError> {
    let mut sites = Vec::new();
    let mut current = None;
    let mut global = None;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index as u64 + 1;
        let line = strip_inline_comment(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("<virtualhost") {
            let mut site = new_site(
                "apache",
                format!("apache vhost line {line_number}"),
                line_number,
            );
            site.listen = virtual_host_listen(trimmed);
            current = Some(site);
        } else if lower.starts_with("</virtualhost") {
            if let Some(site) = current.take() {
                sites.push(site);
            }
        } else if let Some(site) = current.as_mut() {
            collect_apache_directive(site, trimmed);
        } else if is_apache_site_directive(trimmed) {
            let site = global.get_or_insert_with(|| {
                new_site("apache", "apache global".to_string(), line_number)
            });
            collect_apache_directive(site, trimmed);
        }
    }
    if let Some(site) = current {
        sites.push(site);
    }
    if let Some(site) = global {
        sites.push(site);
    }
    Ok(sites)
}

fn new_site(server_kind: &str, site_name: String, line_number: u64) -> WebSite {
    WebSite {
        server_kind: server_kind.to_string(),
        site_name,
        hostnames: Vec::new(),
        listen: Vec::new(),
        document_roots: Vec::new(),
        access_logs: Vec::new(),
        error_logs: Vec::new(),
        line_number,
    }
}

fn finish_closed_site(current: &mut Option<WebSite>, sites: &mut Vec<WebSite>, depth: i32) {
    if depth <= 0 {
        if let Some(site) = current.take() {
            sites.push(site);
        }
    }
}

fn collect_nginx_directive(site: &mut WebSite, line: &str) {
    let line = line.trim_end_matches(';').trim();
    if let Some(rest) = line.strip_prefix("listen ") {
        push_tokens(&mut site.listen, rest);
    } else if let Some(rest) = line.strip_prefix("server_name ") {
        push_tokens(&mut site.hostnames, rest);
    } else if let Some(rest) = line.strip_prefix("root ") {
        push_first_token(&mut site.document_roots, rest);
    } else if let Some(rest) = line.strip_prefix("access_log ") {
        push_first_token(&mut site.access_logs, rest);
    } else if let Some(rest) = line.strip_prefix("error_log ") {
        push_first_token(&mut site.error_logs, rest);
    }
}

fn collect_apache_directive(site: &mut WebSite, line: &str) {
    let mut parts = line.split_whitespace();
    let Some(key) = parts.next() else {
        return;
    };
    let rest = parts.collect::<Vec<_>>().join(" ");
    match key.to_ascii_lowercase().as_str() {
        "servername" => push_first_token(&mut site.hostnames, &rest),
        "serveralias" => push_tokens(&mut site.hostnames, &rest),
        "documentroot" => push_first_token(&mut site.document_roots, &rest),
        "customlog" | "transferlog" => push_first_token(&mut site.access_logs, &rest),
        "errorlog" => push_first_token(&mut site.error_logs, &rest),
        "listen" => push_first_token(&mut site.listen, &rest),
        _ => {}
    }
}

fn is_apache_site_directive(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("servername ")
        || lower.starts_with("serveralias ")
        || lower.starts_with("documentroot ")
        || lower.starts_with("customlog ")
        || lower.starts_with("transferlog ")
        || lower.starts_with("errorlog ")
        || lower.starts_with("listen ")
}
