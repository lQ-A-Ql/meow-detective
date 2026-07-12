use super::*;

#[test]
fn parse_empty_data() {
    let result = parse_cloudtrail("");
    assert!(result.is_err());
}

#[test]
fn parse_cloudtrail_wrapped_records() {
    let json = r#"{
        "Records": [
            {
                "eventVersion": "1.08",
                "userIdentity": {
                    "type": "IAMUser",
                    "arn": "arn:aws:iam::123456789012:user/alice",
                    "userName": "alice"
                },
                "eventTime": "2024-06-15T12:00:00Z",
                "eventSource": "s3.amazonaws.com",
                "eventName": "PutObject",
                "awsRegion": "us-east-1",
                "sourceIPAddress": "203.0.113.1",
                "resources": [
                    {
                        "ARN": "arn:aws:s3:::my-bucket/key.txt",
                        "type": "AWS::S3::Object"
                    }
                ]
            }
        ]
    }"#;

    let entries = parse_cloudtrail(json).expect("should parse");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.action, "s3:PutObject");
    assert_eq!(
        entry.principal.as_deref(),
        Some("arn:aws:iam::123456789012:user/alice")
    );
    assert_eq!(
        entry.target.as_deref(),
        Some("arn:aws:s3:::my-bucket/key.txt")
    );
    assert_eq!(entry.timestamp.as_deref(), Some("2024-06-15T12:00:00Z"));
    assert!(entry.raw.is_some());
}

#[test]
fn parse_cloudtrail_json_lines() {
    let json = r#"{"eventVersion":"1.08","userIdentity":{"arn":"arn:aws:iam::123456789012:user/bob"},"eventTime":"2024-06-15T12:05:00Z","eventSource":"ec2.amazonaws.com","eventName":"DescribeInstances","awsRegion":"us-west-2"}
{"eventVersion":"1.08","userIdentity":{"userName":"charlie"},"eventTime":"2024-06-15T12:10:00Z","eventSource":"iam.amazonaws.com","eventName":"CreateUser","awsRegion":"us-east-1"}"#;

    let entries = parse_cloudtrail(json).expect("should parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "ec2:DescribeInstances");
    assert_eq!(entries[1].action, "iam:CreateUser");
}

#[test]
fn parse_cloudtrail_minimal_record() {
    let json = r#"{"Records":[{"eventName":"SignOut"}]}"#;
    let entries = parse_cloudtrail(json).expect("should parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "SignOut");
    assert!(entries[0].principal.is_none());
}
