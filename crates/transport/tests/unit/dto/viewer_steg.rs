use super::*;

fn completed_dto() -> ImageStegDto {
    ImageStegDto {
        status: ImageStegStatusDto::Completed,
        format: Some("png".to_string()),
        width: Some(640),
        height: Some(480),
        reason: None,
        global: Some(ImageStegAssessmentDto {
            score: Some(41.5),
            band: ImageStegBandDto::High,
            methods_used: 3,
            methods: vec![
                ImageStegMethodDto {
                    method: ImageStegMethodKindDto::ChiSquare,
                    estimate: Some(0.7075),
                    score: Some(41.5),
                    samples: 307_200,
                },
                ImageStegMethodDto {
                    method: ImageStegMethodKindDto::Rs,
                    estimate: None,
                    score: None,
                    samples: 307_200,
                },
            ],
        }),
        dominant_plane: Some("g".to_string()),
        planes: vec![ImageStegPlaneDto {
            plane: "g".to_string(),
            samples: 307_200,
            unique_values: 256,
            assessment: None,
        }],
        blocks: vec![ImageStegBlockDto {
            x: 64,
            y: 128,
            width: 64,
            height: 64,
            plane: "g".to_string(),
            score: 55.25,
            band: ImageStegBandDto::High,
            methods_used: 3,
        }],
        blocks_scored: 70,
        blocks_total: 80,
        warnings: vec![ImageStegWarningDto {
            code: "image.steg.single-method".to_string(),
            severity: ImageIssueSeverityDto::Warning,
            message: "Plane r produced fewer than two methods.".to_string(),
        }],
    }
}

#[test]
fn steg_dto_round_trips_with_camel_case() {
    let dto = completed_dto();
    let json = serde_json::to_value(&dto).unwrap();

    assert_eq!(json["status"], "completed");
    assert_eq!(json["format"], "png");
    assert_eq!(json["dominantPlane"], "g");
    assert_eq!(json["blocksScored"], 70);
    assert_eq!(json["blocksTotal"], 80);
    assert_eq!(json["global"]["band"], "high");
    assert_eq!(json["global"]["methodsUsed"], 3);
    assert_eq!(json["global"]["methods"][0]["method"], "chiSquare");
    assert_eq!(json["global"]["methods"][1]["method"], "rs");
    assert!(json["global"]["methods"][1].get("estimate").is_none());
    assert_eq!(json["planes"][0]["uniqueValues"], 256);
    assert_eq!(json["blocks"][0]["plane"], "g");
    assert_eq!(json["warnings"][0]["severity"], "warning");

    let round_trip: ImageStegDto = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, dto);
}

#[test]
fn steg_status_only_failure_skips_optional_fields() {
    let dto = ImageStegDto {
        status: ImageStegStatusDto::Oversized,
        format: Some("bmp".to_string()),
        width: None,
        height: None,
        reason: Some("limit.bytes".to_string()),
        global: None,
        dominant_plane: None,
        planes: Vec::new(),
        blocks: Vec::new(),
        blocks_scored: 0,
        blocks_total: 0,
        warnings: Vec::new(),
    };
    let json = serde_json::to_value(&dto).unwrap();

    assert_eq!(json["status"], "oversized");
    assert_eq!(json["reason"], "limit.bytes");
    for field in [
        "width",
        "height",
        "global",
        "dominantPlane",
        "planes",
        "blocks",
        "warnings",
    ] {
        assert!(json.get(field).is_none(), "unexpected field {field}");
    }

    let round_trip: ImageStegDto = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, dto);
}

#[test]
fn steg_status_values_are_stable() {
    let cases = [
        (ImageStegStatusDto::Completed, "completed"),
        (ImageStegStatusDto::Unsupported, "unsupported"),
        (ImageStegStatusDto::Oversized, "oversized"),
        (ImageStegStatusDto::Corrupt, "corrupt"),
    ];
    for (value, expected) in cases {
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::Value::String(expected.to_string())
        );
    }

    let bands = [
        (ImageStegBandDto::Low, "low"),
        (ImageStegBandDto::Medium, "medium"),
        (ImageStegBandDto::High, "high"),
        (ImageStegBandDto::Inconclusive, "inconclusive"),
    ];
    for (value, expected) in bands {
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::Value::String(expected.to_string())
        );
    }

    let methods = [
        (ImageStegMethodKindDto::ChiSquare, "chiSquare"),
        (ImageStegMethodKindDto::Rs, "rs"),
        (ImageStegMethodKindDto::Spa, "spa"),
    ];
    for (value, expected) in methods {
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::Value::String(expected.to_string())
        );
    }
}
