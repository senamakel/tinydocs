//! The `.pptx` presentation spec: the typed description a caller hands to
//! `pptx::generate`, plus the size limits every spec is validated against.
//!
//! Same contract rules as [`crate::spec::document`] — `deny_unknown_fields`,
//! public limits, `validate` before synthesis — with one structural difference
//! worth understanding.
//!
//! # Images are bytes here, not references
//!
//! A [`SlideImage`] carries the image *bytes*, its format, and its native pixel
//! dimensions. It deliberately does **not** carry a path, a URL, or an
//! application-specific identifier, because resolving any of those is host
//! policy this crate has no business holding: which directories an agent may
//! read, whether a given identifier belongs to the caller, and whether fetching
//! a URL is an acceptable request to originate are all questions with different
//! answers in every host. A host resolves indirection under its own rules and
//! hands over the resulting bytes.
//!
//! The serialized image facts are vocabulary only. The `TinyDocs` implementation
//! interprets encoded headers and exposes that operation through its module;
//! this contract validates declared fields and size limits only.

use serde::{Deserialize, Serialize};

use crate::spec::image::ImageFormat;
use crate::{Error, Result};

/// Maximum number of content slides a single deck may contain.
///
/// Bounds generation time and output size; a caller with more material is
/// expected to split it across multiple decks.
pub const MAX_SLIDES: usize = 64;

/// Maximum length, in Unicode scalar values, of any single text field — the
/// deck title, the author byline, the theme hint, a slide title, a slide body,
/// one bullet, the speaker notes, or an image caption.
pub const MAX_TEXT_CHARS: usize = 2_000;

/// Maximum number of bullets on a single slide.
///
/// Higher counts produce a slide nobody can read, and bloat the output.
pub const MAX_BULLETS_PER_SLIDE: usize = 32;

/// Maximum number of images attached to a single slide.
///
/// The single-column layout stacks images vertically in the lower band of the
/// slide; past this count each one is too small to read.
pub const MAX_IMAGES_PER_SLIDE: usize = 6;

/// Maximum number of images across the whole deck.
///
/// Bounds the embedded media payload regardless of how the images are
/// distributed across slides.
pub const MAX_IMAGES_PER_DECK: usize = 8;

/// Maximum size, in bytes, of a single embedded image.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// One image embedded on a slide.
///
/// The implementation derives and verifies `format` and dimensions from the
/// bytes before synthesis. The contract does not parse encoded headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlideImage {
    /// The encoded image, as PNG or JPEG bytes.
    pub bytes: Vec<u8>,
    /// The format of `bytes`.
    pub format: ImageFormat,
    /// Native width in pixels, used to place the image without distorting it.
    pub width_px: u32,
    /// Native height in pixels, used to place the image without distorting it.
    pub height_px: u32,
    /// Optional caption, rendered as a bullet beneath the image.
    #[serde(default)]
    pub caption: Option<String>,
}

/// One content slide of the deck, rendered in spec order.
///
/// At least one of `title`, `body`, or `bullets` must carry renderable text.
/// Images alone are not enough — a slide holding only an image and no label
/// reads as a rendering bug rather than a design choice, and synthesis drops
/// blank text anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlideSpec {
    /// Slide title. May be blank for a visually minimal slide, as long as the
    /// body or bullets carry text.
    #[serde(default)]
    pub title: String,
    /// Body text, rendered above the bullets. Plain text only.
    #[serde(default)]
    pub body: Option<String>,
    /// Bullets, rendered after the body text.
    #[serde(default)]
    pub bullets: Vec<String>,
    /// Speaker notes attached to the slide.
    #[serde(default)]
    pub speaker_notes: Option<String>,
    /// Images, stacked in a single column beneath the text.
    #[serde(default)]
    pub images: Vec<SlideImage>,
}

impl SlideSpec {
    /// Builds a slide from its text fields and already-resolved images.
    ///
    /// Hosts resolve their own image sources (artifacts, workspace files)
    /// under their own policy and hand the contract only the bytes.
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        body: Option<String>,
        bullets: Vec<String>,
        speaker_notes: Option<String>,
        images: Vec<SlideImage>,
    ) -> Self {
        Self {
            title: title.into(),
            body,
            bullets,
            speaker_notes,
            images,
        }
    }

    /// Returns `true` when the slide carries no renderable text at all — the
    /// title, body, and every bullet are absent or blank.
    ///
    /// Synthesis trims and drops blank entries, so a slide holding only
    /// `["   "]` would render without text despite carrying entries.
    #[must_use]
    pub fn is_textless(&self) -> bool {
        let has_title = !self.title.trim().is_empty();
        let has_body = self.body.as_deref().is_some_and(|b| !b.trim().is_empty());
        let has_bullets = self.bullets.iter().any(|b| !b.trim().is_empty());
        !(has_title || has_body || has_bullets)
    }
}

/// Metadata for one slide in a presentation request, suitable for validation
/// before a host resolves its image sources.
///
/// Each entry represents one requested image and carries only its optional
/// caption. It deliberately contains no source identifier, path, URL, bytes,
/// or decoded image facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlideRequestSpec {
    /// Slide title.
    #[serde(default)]
    pub title: String,
    /// Body text, rendered above the bullets.
    #[serde(default)]
    pub body: Option<String>,
    /// Bullets, rendered after the body text.
    #[serde(default)]
    pub bullets: Vec<String>,
    /// Speaker notes attached to the slide.
    #[serde(default)]
    pub speaker_notes: Option<String>,
    /// Captions in requested-image order; `None` represents an uncaptained
    /// image without exposing the host's source reference.
    #[serde(default)]
    pub images: Vec<Option<String>>,
}

/// A source-agnostic presentation request suitable for validation before
/// resolving image sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationRequestSpec {
    /// Deck title, rendered on a leading title slide.
    pub title: String,
    /// Optional author byline.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional theme hint.
    #[serde(default)]
    pub theme: Option<String>,
    /// Content slides, in display order.
    #[serde(default)]
    pub slides: Vec<SlideRequestSpec>,
}

impl PresentationRequestSpec {
    /// Validate all shared text and cardinality constraints without requiring
    /// resolved image bytes. Hosts should call this before doing image IO.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] naming the first invalid field.
    pub fn validate(&self) -> Result<()> {
        validate_deck_fields(
            &self.title,
            self.author.as_deref(),
            self.theme.as_deref(),
            self.slides.len(),
            self.slides.iter().map(|slide| slide.images.len()),
        )?;

        for (i, slide) in self.slides.iter().enumerate() {
            validate_slide_text(
                i,
                &slide.title,
                slide.body.as_deref(),
                &slide.bullets,
                slide.speaker_notes.as_deref(),
            )?;
            validate_slide_image_count(i, slide.images.len())?;
            for (m, caption) in slide.images.iter().enumerate() {
                validate_caption(i, m, caption.as_deref())?;
            }
        }
        Ok(())
    }
}

fn validate_deck_fields(
    title: &str,
    author: Option<&str>,
    theme: Option<&str>,
    slide_count: usize,
    image_counts: impl Iterator<Item = usize>,
) -> Result<()> {
    if title.trim().is_empty() {
        return Err(Error::invalid_input("title", "must not be empty"));
    }
    check_text_len("title", title)?;
    if let Some(author) = author {
        check_text_len("author", author)?;
    }
    if let Some(theme) = theme {
        check_text_len("theme", theme)?;
    }
    if slide_count == 0 {
        return Err(Error::invalid_input(
            "slides",
            "must contain at least one slide",
        ));
    }
    if slide_count > MAX_SLIDES {
        return Err(Error::invalid_input(
            "slides",
            format!("must contain ≤ {MAX_SLIDES} slides"),
        ));
    }
    if image_count_over_limit(image_counts) {
        return Err(Error::invalid_input(
            "slides[].images",
            format!("deck must contain ≤ {MAX_IMAGES_PER_DECK} images total"),
        ));
    }

    Ok(())
}

fn image_count_over_limit(counts: impl Iterator<Item = usize>) -> bool {
    let mut count = 0usize;
    for slide_count in counts {
        count = count.saturating_add(slide_count);
        if count > MAX_IMAGES_PER_DECK {
            return true;
        }
    }
    false
}

fn validate_slide_text(
    i: usize,
    title: &str,
    body: Option<&str>,
    bullets: &[String],
    speaker_notes: Option<&str>,
) -> Result<()> {
    let is_textless = title.trim().is_empty()
        && body.is_none_or(|value| value.trim().is_empty())
        && !bullets.iter().any(|bullet| !bullet.trim().is_empty());
    if is_textless {
        return Err(Error::invalid_input(
            format!("slides[{i}]"),
            "must have at least one of title / body / bullets",
        ));
    }
    check_text_len(format!("slides[{i}].title"), title)?;
    if let Some(body) = body {
        check_text_len(format!("slides[{i}].body"), body)?;
    }
    if bullets.len() > MAX_BULLETS_PER_SLIDE {
        return Err(Error::invalid_input(
            format!("slides[{i}].bullets"),
            format!("must contain ≤ {MAX_BULLETS_PER_SLIDE} bullets"),
        ));
    }
    for (b, bullet) in bullets.iter().enumerate() {
        check_text_len(format!("slides[{i}].bullets[{b}]"), bullet)?;
    }
    if let Some(notes) = speaker_notes {
        check_text_len(format!("slides[{i}].speaker_notes"), notes)?;
    }
    Ok(())
}

fn validate_slide_image_count(i: usize, image_count: usize) -> Result<()> {
    if image_count > MAX_IMAGES_PER_SLIDE {
        return Err(Error::invalid_input(
            format!("slides[{i}].images"),
            format!("must contain ≤ {MAX_IMAGES_PER_SLIDE} images"),
        ));
    }
    Ok(())
}

fn validate_caption(i: usize, m: usize, caption: Option<&str>) -> Result<()> {
    if let Some(caption) = caption {
        check_text_len(format!("slides[{i}].images[{m}].caption"), caption)?;
    }
    Ok(())
}

fn check_text_len(field: impl Into<String>, value: &str) -> Result<()> {
    if value.chars().count() > MAX_TEXT_CHARS {
        return Err(Error::invalid_input(
            field,
            format!("must be ≤ {MAX_TEXT_CHARS} chars"),
        ));
    }
    Ok(())
}

/// A complete `.pptx` presentation spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationSpec {
    /// Deck title, rendered on a leading title slide. Required and non-blank.
    pub title: String,
    /// Optional author byline, rendered beneath the deck title.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional theme hint.
    ///
    /// Accepted and validated but not yet acted on: synthesis uses the writer's
    /// default template regardless. It is part of the contract so a host's tool
    /// schema does not have to change when template selection lands.
    #[serde(default)]
    pub theme: Option<String>,
    /// Content slides, in display order. Must contain at least one entry.
    #[serde(default)]
    pub slides: Vec<SlideSpec>,
}

impl PresentationSpec {
    /// Total number of images across every slide.
    #[must_use]
    pub fn image_count(&self) -> usize {
        self.slides
            .iter()
            .map(|slide| slide.images.len())
            .sum::<usize>()
    }

    /// Check the spec against every documented size limit and declared-value
    /// invariant. The implementation interprets encoded image headers.
    ///
    /// The contract validates structure and declared fields. It does not parse
    /// encoded image headers; `tinydocs::pptx::generate` checks those before
    /// synthesis, and hosts can use the module's `InspectImage` operation when
    /// they need image facts at their own boundary.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] naming the first field that violates a
    /// limit. Fields are checked in spec order, so the reported field is stable
    /// for a given spec.
    pub fn validate(&self) -> Result<()> {
        validate_deck_fields(
            &self.title,
            self.author.as_deref(),
            self.theme.as_deref(),
            self.slides.len(),
            self.slides.iter().map(|slide| slide.images.len()),
        )?;
        for (i, slide) in self.slides.iter().enumerate() {
            validate_slide_text(
                i,
                &slide.title,
                slide.body.as_deref(),
                &slide.bullets,
                slide.speaker_notes.as_deref(),
            )?;
            validate_slide_image_count(i, slide.images.len())?;
            for (m, image) in slide.images.iter().enumerate() {
                Self::check_image(&format!("slides[{i}].images[{m}]"), image)?;
                validate_caption(i, m, image.caption.as_deref())?;
            }
        }
        Ok(())
    }

    /// Check image fields that do not require interpreting encoded bytes.
    fn check_image(field: &str, image: &SlideImage) -> Result<()> {
        if image.bytes.is_empty() {
            return Err(Error::invalid_input(
                format!("{field}.bytes"),
                "must not be empty",
            ));
        }
        if image.bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::invalid_input(
                format!("{field}.bytes"),
                format!("must be ≤ {MAX_IMAGE_BYTES} bytes"),
            ));
        }
        if image.width_px == 0 || image.height_px == 0 {
            return Err(Error::invalid_input(
                format!("{field}.width_px"),
                "width and height must be greater than zero",
            ));
        }
        Ok(())
    }
}

pub mod wire;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
