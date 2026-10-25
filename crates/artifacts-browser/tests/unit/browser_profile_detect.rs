use super::*;

#[test]
fn detect_chrome_default_profile() {
    let paths =
        vec!["/Users/john/AppData/Local/Google/Chrome/User Data/Default/History".to_string()];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].browser, "Chrome");
    assert_eq!(profiles[0].profile_name, "Default");
}

#[test]
fn detect_chrome_multiple_profiles() {
    let paths = vec![
        "/users/john/appdata/local/google/chrome/user data/default/history".to_string(),
        "/users/john/appdata/local/google/chrome/user data/profile 1/history".to_string(),
        "/users/john/appdata/local/google/chrome/user data/profile 2/cookies".to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 3);
    let names: Vec<&str> = profiles.iter().map(|p| p.profile_name.as_str()).collect();
    assert!(names.contains(&"Default"));
    assert!(names.contains(&"Profile 1"));
    assert!(names.contains(&"Profile 2"));
}

#[test]
fn detect_edge_profiles() {
    let paths = vec![
        "/users/john/appdata/local/microsoft/edge/user data/default/history".to_string(),
        "/users/john/appdata/local/microsoft/edge/user data/profile 1/history".to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].browser, "Edge");
}

#[test]
fn detect_firefox_profiles() {
    let paths = vec![
        "/Users/john/AppData/Roaming/Mozilla/Firefox/Profiles/abc123.default-release/places.sqlite"
            .to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].browser, "Firefox");
    assert_eq!(profiles[0].profile_name, "default-release");
}

#[test]
fn detect_mixed_browsers() {
    let paths = vec![
        "/users/john/appdata/local/google/chrome/user data/default/history".to_string(),
        "/users/john/appdata/local/microsoft/edge/user data/default/history".to_string(),
        "/users/john/appdata/roaming/mozilla/firefox/profiles/abc.default/places.sqlite"
            .to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 3);
    let browsers: Vec<&str> = profiles.iter().map(|p| p.browser.as_str()).collect();
    assert!(browsers.contains(&"Chrome"));
    assert!(browsers.contains(&"Edge"));
    assert!(browsers.contains(&"Firefox"));
}

#[test]
fn detect_no_browser_paths() {
    let paths = vec![
        "/windows/system32/config/system".to_string(),
        "/users/john/documents/report.docx".to_string(),
    ];
    assert!(detect_browser_profiles(&paths).is_empty());
}

#[test]
fn detect_empty_input() {
    assert!(detect_browser_profiles(&[]).is_empty());
}

#[test]
fn detect_deduplicates_duplicate_paths() {
    let paths = vec![
        "/users/john/appdata/local/google/chrome/user data/default/history".to_string(),
        "/users/john/appdata/local/google/chrome/user data/default/cookies".to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].profile_name, "Default");
}

#[test]
fn detect_chrome_archived_history() {
    let paths = vec![
        "/users/john/appdata/local/google/chrome/user data/default/Archived History".to_string(),
    ];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].browser, "Chrome");
    assert_eq!(profiles[0].profile_name, "Default");
}

#[test]
fn detect_chrome_last_session() {
    let paths =
        vec!["/users/john/appdata/local/google/chrome/user data/default/Last Session".to_string()];
    let profiles = detect_browser_profiles(&paths);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].browser, "Chrome");
}

#[test]
fn detect_handles_windows_backslashes() {
    let paths = vec![
        "\\Users\\john\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\History".to_string(),
        "\\Users\\john\\AppData\\Roaming\\Mozilla\\Firefox\\Profiles\\abc.default\\places.sqlite"
            .to_string(),
    ];
    assert_eq!(detect_browser_profiles(&paths).len(), 2);
}

#[test]
fn firefox_dir_to_name_single_word() {
    assert_eq!(
        firefox_dir_to_name("/mozilla/firefox/profiles/abc.default"),
        "default"
    );
}

#[test]
fn firefox_dir_to_name_hyphenated() {
    assert_eq!(
        firefox_dir_to_name("/mozilla/firefox/profiles/xyz.default-release"),
        "default-release"
    );
}

#[test]
fn firefox_dir_to_name_no_dot() {
    assert_eq!(
        firefox_dir_to_name("/mozilla/firefox/profiles/simple"),
        "simple"
    );
}

#[test]
fn chromelike_dir_to_name_default() {
    assert_eq!(chromelike_dir_to_name("default"), "Default");
}

#[test]
fn chromelike_dir_to_name_profile_with_space() {
    assert_eq!(chromelike_dir_to_name("profile 1"), "Profile 1");
}

#[test]
fn chromelike_dir_to_name_with_underscore() {
    assert_eq!(chromelike_dir_to_name("guest_profile"), "Guest Profile");
}
