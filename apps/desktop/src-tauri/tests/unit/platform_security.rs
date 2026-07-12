use super::*;
use std::io::Write;

#[test]
fn restrict_succeeds_for_regular_file() {
    let dir = std::env::temp_dir().join(format!(
        "forensics-platform-security-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("restricted.txt");
    {
        let mut f = std::fs::File::create(&file).unwrap();
        f.write_all(b"secret").unwrap();
    }

    let result = restrict_file_to_current_user(&file);
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        result.is_ok(),
        "restrict_file_to_current_user failed: {result:?}"
    );
}
