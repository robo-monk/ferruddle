use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
/// Extraction settings. Defaults enable cleanup and preserve article images.
///
/// Sanitization is unconditional; disabling cleanup never disables it.
pub struct Options {
    /// Add Markdown alongside cleaned HTML. Default: `false`.
    pub markdown: bool,
    /// Optional root selector; invalid or unmatched selectors use automatic selection.
    pub content_selector: Option<String>,
    /// Remove known clutter selectors. Default: `true`.
    pub remove_exact_selectors: bool,
    /// Remove clutter matched by partial attribute values. Default: `true`.
    pub remove_partial_selectors: bool,
    /// Remove hidden elements, with content-recovery retries. Default: `true`.
    pub remove_hidden_elements: bool,
    /// Remove blocks scored as clutter. Default: `true`.
    pub remove_low_scoring: bool,
    /// Remove icons and unresolved tiny placeholders. Default: `true`.
    pub remove_small_images: bool,
    /// Remove repeated navigation, metadata and related-content patterns. Default: `true`.
    pub remove_content_patterns: bool,
    /// Remove article and adapter-produced images. Default: `false`.
    pub remove_images: bool,
    /// Normalize code, headings, images and other structure. Default: `true`.
    pub standardize: bool,
    /// Select replies already available to offline site adapters.
    pub include_replies: IncludeReplies,
    /// Return diagnostics and retain diagnostic attributes. Default: `false`.
    pub debug: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            markdown: false,
            content_selector: None,
            remove_exact_selectors: true,
            remove_partial_selectors: true,
            remove_hidden_elements: true,
            remove_low_scoring: true,
            remove_small_images: true,
            remove_content_patterns: true,
            remove_images: false,
            standardize: true,
            include_replies: IncludeReplies::Extractors,
            debug: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Selection of local replies. No variant enables network requests.
pub enum IncludeReplies {
    #[default]
    /// Let the offline site adapter select available replies.
    Extractors,
    /// Currently the same offline selection as [`Self::Extractors`].
    All,
    /// Exclude replies.
    None,
}

/// Untrusted strings copied from the page; escape or validate before rendering.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Metadata {
    /// Article title, or an empty string when unavailable.
    pub title: String,
    /// Page description.
    pub description: String,
    /// Page domain when URL context is available.
    pub domain: String,
    /// Authored favicon URL; validate before using it elsewhere.
    pub favicon: String,
    /// Authored representative image URL; validate before using it elsewhere.
    pub image: String,
    /// Page language declaration.
    pub language: String,
    /// Publication date when available.
    pub published: String,
    /// Article author.
    pub author: String,
    /// Site name.
    pub site: String,
    /// Parsed schema.org data from the page.
    pub schema_org_data: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Main-content extraction result. JSON flattens metadata and uses camel case.
pub struct Extraction {
    /// Cleaned HTML, including when Markdown is requested.
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Markdown when [`Options::markdown`] is enabled.
    pub content_markdown: Option<String>,
    #[serde(flatten)]
    /// Untrusted page metadata, separate from cleaned article content.
    pub metadata: Metadata,
    /// Cleaned-content word count with structural block boundaries.
    pub word_count: usize,
    /// Selected site-adapter name, when used.
    pub extractor_type: Option<String>,
    /// Nonfatal limitations such as detected unsupported math.
    pub warnings: Vec<Warning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Selection and removal diagnostics when [`Options::debug`] is enabled.
    pub debug: Option<DebugInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Diagnostic information requested through [`Options::debug`].
pub struct DebugInfo {
    /// Selector identifying the selected content root.
    pub content_selector: String,
    /// Content removed by cleanup stages.
    pub removals: Vec<DebugRemoval>,
    /// Recoverable diagnostics collected during extraction.
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// One cleanup removal recorded in debug mode.
pub struct DebugRemoval {
    /// Name of the cleanup stage.
    pub step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Selector responsible for removal, when applicable.
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Human-readable removal reason, when available.
    pub reason: Option<String>,
    /// Removed page text, which remains untrusted input.
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
/// Stable machine-readable warning codes.
pub enum WarningCode {
    /// Math was detected, but this library does not convert formulas.
    MathNotSupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// A nonfatal extraction or conversion limitation.
pub struct Warning {
    /// Machine-readable code; math serializes as `math_not_supported`.
    pub code: WarningCode,
    /// Human-readable explanation.
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Result of standalone [`crate::html_to_markdown`] conversion.
pub struct MarkdownOutput {
    /// Markdown converted from the supplied HTML.
    pub markdown: String,
    /// Nonfatal limitations detected in that HTML.
    pub warnings: Vec<Warning>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
/// An extraction or serialization failure.
pub enum Error {
    #[error("Invalid input: {0}")]
    /// Input could not be accepted.
    InvalidInput(String),
    #[error("Could not serialize HTML: {0}")]
    /// Cleaned HTML could not be serialized.
    Serialization(String),
}
