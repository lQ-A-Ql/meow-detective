/// Built-in V2 correlation rules.
pub const V2_STANDARD_TOML: &str = r#"
[manifest]
name = "v2-standard"
version = "1.0.0"
author = "Forensics Workbench"
description = "Built-in V2 correlation rules mapping artifacts to files"
scope = ["correlation", "investigation"]
min_product_version = "0.1.0"
caveats = [
    "Path-based matches depend on artifact field normalization",
    "Name-based matches may hit unrelated files with the same basename"
]

[[rules]]
id = "lnk-path-match"
name = "LNK Target Path Match"
description = "Match LNK artifact target_path to file entries by exact path"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "target_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "May need to review original LNK target_path content"

[[rules]]
id = "prefetch-name-match"
name = "Prefetch Executable Name Match"
description = "Match Prefetch artifact executable basename to file entries by name"
source_type = "artifact"
source_family = "Prefetch"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "executable"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "strong"
caveats = "Name match may hit files outside the expected binary directory"

[[rules]]
id = "registry-path-match"
name = "Registry Value Path Match"
description = "Match Registry artifact data containing a file path to file entries by path"
source_type = "artifact"
source_family = "RegistryValue"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "data"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "strong"
caveats = "Registry value data may contain env vars or CLI args; review original value"

[[rules]]
id = "recycle-bin-original-path-match"
name = "Recycle Bin Original Path Match"
description = "Match Recycle Bin artifact original_path to deleted file entries by path"
source_type = "artifact"
source_family = "RecycleBin"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "original_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "Original path reflects pre-deletion path; verify deletion time aligns"

[[rules]]
id = "browser-download-path-match"
name = "Browser Download Target Path Match"
description = "Match BrowserDownload artifact targetPath to file entries by path"
source_type = "artifact"
source_family = "BrowserDownload"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "targetPath"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "Download path from browser DB; verify with file content and timeline"

[[rules]]
id = "browser-history-title-name-match"
name = "Browser History Title Name Match"
description = "Match BrowserHistory artifact title to file entries by name"
source_type = "artifact"
source_family = "BrowserHistory"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "title"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Title-based name match is weak; verify with visit time and URL context"

[[rules]]
id = "browser-history-url-name-match"
name = "Browser History URL Name Match"
description = "Match BrowserHistory artifact URL path segment to file entries by name"
source_type = "artifact"
source_family = "BrowserHistory"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "url"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "URL path segment name match is weak; verify with visit time and title"

[[rules]]
id = "email-attachment-name-match"
name = "Email Attachment Name Match"
description = "Match EmailMessage artifact attachment names to file entries by name"
source_type = "artifact"
source_family = "EmailMessage"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "attachments"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Attachment name match is weak; verify with sentAt, subject, and content"

[[rules]]
id = "email-subject-name-match"
name = "Email Subject Name Match"
description = "Match EmailMessage artifact subject to file entries by name"
source_type = "artifact"
source_family = "EmailMessage"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "subject"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "weak"
caveats = "Subject name match is weak; verify with sentAt and attachment context"

[[rules]]
id = "jumplist-path-match"
name = "JumpList Target Path Match"
description = "Match JumpList artifact target_path to file entries by exact path"
source_type = "artifact"
source_family = "JumpList"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "target_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
caveats = "JumpList match depends on embedded LNK extraction quality"
"#;
