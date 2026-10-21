//! Readable HTML and Markdown extraction without network requests or script execution.
//!
//! Math conversion is not included. Detected math is reported through `warnings`.
//!
//! Use [`extract`] to select and clean a page's main content, or
//! [`html_to_markdown`] to convert supplied HTML without article selection.
//! Both APIs are synchronous and accept HTML you have already fetched. A page
//! URL supplies link and site context; it never triggers a request.
//!
//! [`Extraction::content`] always contains HTML. Set [`Options::markdown`] to
//! receive a separate [`Extraction::content_markdown`] value as well. Metadata
//! strings remain untrusted page data; escape or validate them for their intended
//! destination. Sanitization runs regardless of cleanup options.

mod callouts;
mod code;
mod constants;
mod dom;
mod extractors;
mod finalize;
mod footnotes;
mod images;
mod markdown;
mod math;
mod metadata;
mod patterns;
mod pipeline;
mod removals;
mod safety;
mod selection;
mod standardize;
mod types;
mod utils;

pub use types::*;

/// Extract a page's main content. The URL supplies context for links and site rules.
///
/// ```
/// let result = defuddle::extract(
///     "<article><p>A useful article paragraph.</p></article>",
///     Some("https://example.com/article"),
///     &defuddle::Options { markdown: true, ..Default::default() },
/// ).unwrap();
/// assert!(result.content.contains("<p>"));
/// assert_eq!(result.content_markdown.as_deref(), Some("A useful article paragraph."));
/// ```
pub fn extract(html: &str, page_url: Option<&str>, options: &Options) -> Result<Extraction, Error> {
    pipeline::extract(html, page_url, options)
}

/// Convert supplied HTML without selecting its main content. Static declarative
/// shadow content is included. Relative destinations use the page URL and an
/// authored `<base href>` when available; missing/invalid URLs keep relative
/// values. This dialect omits a leading H1 article title, as extraction does.
/// Detected math is kept as safe source with a warning; it is not converted.
pub fn html_to_markdown(html: &str, page_url: Option<&str>) -> Result<MarkdownOutput, Error> {
    markdown::convert(html, page_url)
}

#[cfg(test)]
extern crate self as defuddle;

#[cfg(test)]
#[path = "../tests/internal/mod.rs"]
mod tests;
