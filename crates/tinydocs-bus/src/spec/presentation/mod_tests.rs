//! Unit tests for the presentation wire contract.
//!
//! Format-independent, like the spec itself: these must pass in a build with
//! every format feature off.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    MAX_BULLETS_PER_SLIDE, MAX_IMAGE_BYTES, MAX_IMAGES_PER_DECK, MAX_IMAGES_PER_SLIDE, MAX_SLIDES,
    MAX_TEXT_CHARS, PresentationRequestSpec, PresentationSpec, SlideImage, SlideRequestSpec,
    SlideSpec,
};
use crate::Error;
use crate::spec::image::ImageFormat;

/// One valid slide carrying a title, a body, and a bullet.
fn slide() -> SlideSpec {
    SlideSpec {
        title: "Overview".to_string(),
        body: Some("The situation so far.".to_string()),
        bullets: vec!["A bullet".to_string()],
        speaker_notes: Some("Keep it short.".to_string()),
        images: vec![],
    }
}

/// A minimal valid spec; each test mutates one field to drive a single branch.
fn spec() -> PresentationSpec {
    PresentationSpec {
        title: "Quarterly Review".to_string(),
        author: Some("Alice".to_string()),
        theme: Some("plain".to_string()),
        slides: vec![slide()],
    }
}

fn request_spec() -> PresentationRequestSpec {
    PresentationRequestSpec {
        title: "Quarterly Review".to_string(),
        author: Some("Alice".to_string()),
        theme: Some("plain".to_string()),
        slides: vec![SlideRequestSpec {
            title: "Overview".to_string(),
            body: Some("The situation so far.".to_string()),
            bullets: vec!["A bullet".to_string()],
            speaker_notes: Some("Keep it short.".to_string()),
            images: vec![],
        }],
    }
}

/// A valid image built from real header bytes.
fn image() -> SlideImage {
    SlideImage {
        bytes: vec![1, 2, 3],
        format: ImageFormat::Png,
        width_px: 320,
        height_px: 200,
        caption: Some("A chart".to_string()),
    }
}

/// Assert `spec` is rejected with an `InvalidInput` naming `field`.
fn assert_rejects(spec: &PresentationSpec, field: &str) {
    match spec.validate() {
        Err(Error::InvalidInput { field: f, .. }) => {
            assert_eq!(f, field, "unexpected rejected field");
        }
        other => panic!("expected InvalidInput({field}), got {other:?}"),
    }
}

#[test]
fn accepts_a_well_formed_spec() {
    assert!(spec().validate().is_ok());
}

#[test]
fn accepts_a_spec_with_images() {
    let mut s = spec();
    s.slides[0].images = vec![image()];
    assert!(s.validate().is_ok());
}

#[test]
fn rejects_a_blank_deck_title() {
    let mut s = spec();
    s.title = "   ".to_string();
    assert_rejects(&s, "title");
}

#[test]
fn rejects_over_long_deck_level_text() {
    for (field, mutate) in [("title", 0), ("author", 1), ("theme", 2)] {
        let mut s = spec();
        let long = "x".repeat(MAX_TEXT_CHARS + 1);
        match mutate {
            0 => s.title = long,
            1 => s.author = Some(long),
            _ => s.theme = Some(long),
        }
        assert_rejects(&s, field);
    }
}

#[test]
fn rejects_a_spec_with_no_slides() {
    let mut s = spec();
    s.slides.clear();
    assert_rejects(&s, "slides");
}

#[test]
fn rejects_too_many_slides() {
    let mut s = spec();
    s.slides = vec![slide(); MAX_SLIDES + 1];
    assert_rejects(&s, "slides");
}

#[test]
fn rejects_a_textless_slide() {
    // Every text entry is present but whitespace-only, so synthesis would drop
    // all of them and render an unlabelled slide.
    let mut s = spec();
    s.slides = vec![SlideSpec {
        title: "  ".to_string(),
        body: Some("\t".to_string()),
        bullets: vec![String::new()],
        speaker_notes: None,
        images: vec![],
    }];
    assert_rejects(&s, "slides[0]");
}

#[test]
fn rejects_a_slide_carrying_only_an_image() {
    // Images do not satisfy the "must have text" rule: an unlabelled slide
    // reads as a rendering bug rather than a design choice.
    let mut s = spec();
    s.slides = vec![SlideSpec {
        title: String::new(),
        body: None,
        bullets: vec![],
        speaker_notes: None,
        images: vec![image()],
    }];
    assert_rejects(&s, "slides[0]");
}

#[test]
fn rejects_over_long_slide_text_naming_its_index() {
    let long = || "x".repeat(MAX_TEXT_CHARS + 1);

    let mut s = spec();
    s.slides.push(SlideSpec {
        title: long(),
        ..slide()
    });
    assert_rejects(&s, "slides[1].title");

    let mut s = spec();
    s.slides[0].body = Some(long());
    assert_rejects(&s, "slides[0].body");

    let mut s = spec();
    s.slides[0].bullets = vec!["ok".to_string(), long()];
    assert_rejects(&s, "slides[0].bullets[1]");

    let mut s = spec();
    s.slides[0].speaker_notes = Some(long());
    assert_rejects(&s, "slides[0].speaker_notes");
}

#[test]
fn rejects_too_many_bullets() {
    let mut s = spec();
    s.slides[0].bullets = vec!["b".to_string(); MAX_BULLETS_PER_SLIDE + 1];
    assert_rejects(&s, "slides[0].bullets");
}

#[test]
fn rejects_too_many_images_on_one_slide() {
    let mut s = spec();
    s.slides[0].images = vec![image(); MAX_IMAGES_PER_SLIDE + 1];
    assert_rejects(&s, "slides[0].images");
}

#[test]
fn rejects_too_many_images_across_the_deck() {
    // Each slide is within the per-slide cap; only the deck total is not. The
    // per-slide cap bounds readability, the deck cap bounds the media payload.
    let per_slide = MAX_IMAGES_PER_SLIDE;
    let slides_needed = MAX_IMAGES_PER_DECK / per_slide + 1;
    let mut s = spec();
    s.slides = vec![
        SlideSpec {
            images: vec![image(); per_slide],
            ..slide()
        };
        slides_needed
    ];
    assert!(s.image_count() > MAX_IMAGES_PER_DECK);
    assert_rejects(&s, "slides[].images");
}

#[test]
fn image_count_sums_across_slides() {
    let mut s = spec();
    s.slides = vec![
        SlideSpec {
            images: vec![image(), image()],
            ..slide()
        },
        SlideSpec {
            images: vec![image()],
            ..slide()
        },
    ];
    assert_eq!(s.image_count(), 3);
}

#[test]
fn rejects_an_over_long_image_caption() {
    let mut s = spec();
    let mut img = image();
    img.caption = Some("c".repeat(MAX_TEXT_CHARS + 1));
    s.slides[0].images = vec![img];
    assert_rejects(&s, "slides[0].images[0].caption");
}

#[test]
fn validate_rejects_empty_and_oversize_image_bytes_without_parsing_them() {
    let mut s = spec();
    let mut img = image();
    img.bytes.clear();
    s.slides[0].images = vec![img];
    assert_rejects(&s, "slides[0].images[0].bytes");

    let mut s = spec();
    let mut img = image();
    img.bytes.resize(MAX_IMAGE_BYTES + 1, 0);
    s.slides[0].images = vec![img];
    assert_rejects(&s, "slides[0].images[0].bytes");
}

#[test]
fn validation_keeps_encoded_image_interpretation_out_of_the_contract() {
    let mut s = spec();
    let mut img = image();
    img.bytes = b"not parsed by the vocabulary crate".to_vec();
    img.width_px += 1;
    s.slides[0].images = vec![img];
    assert!(s.validate().is_ok());
}

#[test]
fn request_validation_rejects_excess_images_without_resolved_image_data() {
    let mut request = request_spec();
    request.slides[0].images = vec![None; MAX_IMAGES_PER_SLIDE + 1];
    assert_rejects_request(&request, "slides[0].images");

    let mut request = request_spec();
    request.slides[0].images = vec![None; MAX_IMAGES_PER_DECK + 1];
    assert_rejects_request(&request, "slides[].images");
}

#[test]
fn request_validation_shares_text_limits_with_resolved_spec_validation() {
    let mut request = request_spec();
    request.title = " ".to_string();
    assert_rejects_request(&request, "title");
    assert_request_error_matches_spec(&request);

    let mut request = request_spec();
    request.slides[0].body = Some("x".repeat(MAX_TEXT_CHARS + 1));
    assert_rejects_request(&request, "slides[0].body");
    assert_request_error_matches_spec(&request);
}

#[test]
fn request_validation_checks_captions_without_image_bytes() {
    let mut request = request_spec();
    request.slides[0].images = vec![Some("c".repeat(MAX_TEXT_CHARS + 1))];
    assert_rejects_request(&request, "slides[0].images[0].caption");

    let mut resolved = spec();
    let mut resolved_image = image();
    resolved_image.caption = request.slides[0].images[0].clone();
    resolved.slides[0].images = vec![resolved_image];
    assert_error(
        &resolved,
        "slides[0].images[0].caption",
        &format!("must be ≤ {MAX_TEXT_CHARS} chars"),
    );
}

#[test]
fn request_validation_accepts_metadata_without_source_identifiers() {
    let mut request = request_spec();
    request.slides.push(SlideRequestSpec {
        title: "Appendix".to_string(),
        body: None,
        bullets: vec!["No images required".to_string()],
        speaker_notes: None,
        images: vec![None],
    });
    assert!(request.validate().is_ok());
}

#[test]
fn request_validation_checks_collection_bounds_before_entries() {
    let mut request = request_spec();
    request.slides = vec![
        SlideRequestSpec {
            title: String::new(),
            body: None,
            bullets: vec![],
            speaker_notes: None,
            images: vec![],
        };
        MAX_SLIDES + 1
    ];
    assert_rejects_request(&request, "slides");

    let mut request = request_spec();
    request.slides[0].images = vec![Some("c".repeat(MAX_TEXT_CHARS + 1)); MAX_IMAGES_PER_DECK + 1];
    assert_rejects_request(&request, "slides[].images");
}

#[test]
fn resolved_validation_preserves_first_error_order_for_image_and_later_slide() {
    let mut s = spec();
    let mut invalid_image = image();
    invalid_image.bytes.clear();
    invalid_image.caption = Some("c".repeat(MAX_TEXT_CHARS + 1));
    s.slides[0].images = vec![invalid_image];
    assert_error(&s, "slides[0].images[0].bytes", "must not be empty");

    let mut s = spec();
    let mut invalid_image = image();
    invalid_image.bytes.clear();
    s.slides[0].images = vec![invalid_image];
    s.slides.push(SlideSpec {
        title: String::new(),
        body: None,
        bullets: vec![],
        speaker_notes: None,
        images: vec![],
    });
    assert_error(&s, "slides[0].images[0].bytes", "must not be empty");
}

fn assert_rejects_request(request: &PresentationRequestSpec, field: &str) {
    match request.validate() {
        Err(Error::InvalidInput { field: f, .. }) => {
            assert_eq!(f, field, "unexpected rejected field");
        }
        other => panic!("expected InvalidInput({field}), got {other:?}"),
    }
}

fn assert_error(spec: &PresentationSpec, expected_field: &str, expected_reason: &str) {
    match spec.validate() {
        Err(Error::InvalidInput { field, reason }) => {
            assert_eq!(field, expected_field);
            assert_eq!(reason, expected_reason);
        }
        other => panic!("expected InvalidInput({expected_field}), got {other:?}"),
    }
}

fn assert_request_error_matches_spec(request: &PresentationRequestSpec) {
    let mut resolved = spec();
    resolved.title = request.title.clone();
    resolved.author = request.author.clone();
    resolved.theme = request.theme.clone();
    resolved.slides[0].title = request.slides[0].title.clone();
    resolved.slides[0].body = request.slides[0].body.clone();
    resolved.slides[0].bullets = request.slides[0].bullets.clone();
    resolved.slides[0].speaker_notes = request.slides[0].speaker_notes.clone();
    let request_error = request.validate().expect_err("request should be invalid");
    let resolved_error = resolved.validate().expect_err("spec should be invalid");
    match (request_error, resolved_error) {
        (
            Error::InvalidInput {
                field: request_field,
                reason: request_reason,
            },
            Error::InvalidInput {
                field: resolved_field,
                reason: resolved_reason,
            },
        ) => {
            assert_eq!(request_field, resolved_field);
            assert_eq!(request_reason, resolved_reason);
        }
        (request_error, resolved_error) => {
            panic!("request/spec errors differ: {request_error:?} vs {resolved_error:?}")
        }
    }
}

#[test]
fn validation_rejects_zero_dimensions_as_a_declared_value_error() {
    let mut s = spec();
    let mut img = image();
    img.width_px = 0;
    s.slides[0].images = vec![img];
    assert_rejects(&s, "slides[0].images[0].width_px");
}

#[test]
fn is_textless_reflects_text_presence() {
    assert!(!slide().is_textless());
    assert!(
        SlideSpec {
            title: String::new(),
            body: None,
            bullets: vec![],
            speaker_notes: None,
            images: vec![],
        }
        .is_textless()
    );
    // A title alone is enough.
    assert!(
        !SlideSpec {
            title: "Only a title".to_string(),
            body: None,
            bullets: vec![],
            speaker_notes: None,
            images: vec![],
        }
        .is_textless()
    );
    // So is a body alone, or a bullet alone.
    assert!(
        !SlideSpec {
            title: String::new(),
            body: Some("Body".to_string()),
            bullets: vec![],
            speaker_notes: None,
            images: vec![],
        }
        .is_textless()
    );
    assert!(
        !SlideSpec {
            title: String::new(),
            body: None,
            bullets: vec!["Bullet".to_string()],
            speaker_notes: None,
            images: vec![],
        }
        .is_textless()
    );
}

#[test]
fn spec_round_trips_through_json() {
    let mut s = spec();
    s.slides[0].images = vec![image()];
    let json = serde_json::to_string(&s).expect("serialises");
    let back: PresentationSpec = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(back, s);
    assert!(back.validate().is_ok());
}

#[test]
fn spec_rejects_unknown_json_fields() {
    let json = r#"{"title":"T","slides":[],"tilte":"typo"}"#;
    assert!(serde_json::from_str::<PresentationSpec>(json).is_err());
}

#[test]
fn spec_defaults_optional_fields() {
    let s: PresentationSpec = serde_json::from_str(r#"{"title":"T"}"#).expect("deserialises");
    assert_eq!(s.author, None);
    assert_eq!(s.theme, None);
    assert_eq!(s.slides.len(), 0);
}

#[test]
fn slide_spec_new_matches_literal_and_pins_wire_shape() {
    let image = SlideImage {
        bytes: vec![1, 2, 3],
        format: ImageFormat::Png,
        width_px: 4,
        height_px: 5,
        caption: Some("cap".to_string()),
    };
    let built = SlideSpec::new(
        "T",
        Some("B".to_string()),
        vec!["x".to_string()],
        Some("N".to_string()),
        vec![image.clone()],
    );
    let literal = SlideSpec {
        title: "T".to_string(),
        body: Some("B".to_string()),
        bullets: vec!["x".to_string()],
        speaker_notes: Some("N".to_string()),
        images: vec![image],
    };
    assert_eq!(built, literal);
    let value = serde_json::to_value(&built).unwrap();
    let obj = value.as_object().unwrap();
    let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["body", "bullets", "images", "speaker_notes", "title"]
    );
    let img = value["images"][0].as_object().unwrap();
    let mut ikeys: Vec<_> = img.keys().map(String::as_str).collect();
    ikeys.sort_unstable();
    assert_eq!(
        ikeys,
        ["bytes", "caption", "format", "height_px", "width_px"]
    );
    let back: SlideSpec = serde_json::from_value(value).unwrap();
    assert_eq!(back, built);
}
