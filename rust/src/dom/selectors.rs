//! Conservative indexing for the fixed clutter selector list. Keys only reject
//! impossible matches; the DOM library remains the authority on CSS semantics.
use std::collections::HashMap;

use dom_query::{Matcher, NodeData, NodeRef};

thread_local! {
    static EXACT: SelectorIndex = SelectorIndex::new(crate::constants::EXACT_SELECTORS);
}

pub(super) fn select_exact(root: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    EXACT.with(|index| index.select(root))
}

#[derive(Default)]
struct SelectorIndex {
    matchers: Vec<Matcher>,
    always: Vec<usize>,
    tags: HashMap<String, Vec<usize>>,
    classes: HashMap<String, Vec<usize>>,
    ids: HashMap<String, Vec<usize>>,
    attributes: HashMap<String, Vec<usize>>,
}

impl SelectorIndex {
    fn new(selectors: &[&str]) -> Self {
        let mut index = Self::default();
        for selector in selectors {
            let Ok(matcher) = Matcher::new(selector) else {
                continue;
            };
            let position = index.matchers.len();
            index.matchers.push(matcher);
            let bucket = match necessary_key(selector) {
                Some((b'.', name)) => index.classes.entry(name.to_owned()),
                Some((b'#', name)) => index.ids.entry(name.to_owned()),
                Some((b'[', name)) => index.attributes.entry(name.to_ascii_lowercase()),
                Some((_, name)) => index.tags.entry(name.to_ascii_lowercase()),
                None => {
                    index.always.push(position);
                    continue;
                }
            };
            bucket.or_default().push(position);
        }
        index
    }

    fn matches(&self, node: NodeRef<'_>) -> bool {
        let any = |positions: &[usize]| positions.iter().any(|&i| node.is_match(&self.matchers[i]));
        if any(&self.always) {
            return true;
        }
        node.query_or(false, |node| {
            let NodeData::Element(element) = &node.data else {
                return false;
            };
            if self
                .tags
                .get(element.name.local.to_ascii_lowercase().as_ref())
                .is_some_and(|positions| any(positions))
            {
                return true;
            }
            element.attrs.iter().any(|attribute| {
                let name = attribute.name.local.to_ascii_lowercase();
                if self
                    .attributes
                    .get(name.as_ref())
                    .is_some_and(|positions| any(positions))
                {
                    return true;
                }
                match name.as_ref() {
                    "id" => self
                        .ids
                        .get(attribute.value.as_ref())
                        .is_some_and(|positions| any(positions)),
                    "class" => attribute.value.split_ascii_whitespace().any(|class| {
                        self.classes
                            .get(class)
                            .is_some_and(|positions| any(positions))
                    }),
                    _ => false,
                }
            })
        })
    }

    fn select<'a>(&self, root: NodeRef<'a>) -> Vec<NodeRef<'a>> {
        root.descendants_it()
            .filter(|node| node.is_element() && self.matches(*node))
            .collect()
    }
}

/// Accept only a plain necessary key at the start of a single ASCII compound.
/// Combinators, lists, namespaces, escapes, comments, and unfamiliar syntax fall
/// back to the full matcher. This deliberately does not parse general CSS.
fn necessary_key(selector: &str) -> Option<(u8, &str)> {
    if !selector.is_ascii() || selector.contains('\\') || selector.contains("/*") {
        return None;
    }
    let mut depth = 0usize;
    let mut quote = None;
    for byte in selector.bytes() {
        match (quote, byte) {
            (Some(q), b) if b == q => quote = None,
            (Some(_), _) => (),
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'[' | b'(') => depth += 1,
            (None, b']' | b')') => depth = depth.checked_sub(1)?,
            (None, b) if depth == 0 && (b.is_ascii_whitespace() || b">+~,|".contains(&b)) => {
                return None;
            }
            _ => (),
        }
    }
    if depth != 0 || quote.is_some() {
        return None;
    }
    let first = *selector.as_bytes().first()?;
    let rest = match first {
        b'.' | b'#' | b'[' => &selector[1..],
        b if b.is_ascii_alphabetic() => selector,
        _ => return None,
    };
    let end = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || b"_-".contains(b))
        .count();
    if end == 0 {
        return None;
    }
    let tail = &rest[end..];
    if first == b'[' {
        // A namespace prefix is not an attribute name. `|=` is an operator.
        if tail.starts_with('|') && !tail.starts_with("|=") {
            return None;
        }
    } else if !tail.is_empty() && !tail.starts_with(['.', '#', '[', ':']) {
        return None;
    }
    Some((first, &rest[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom_query::Document;

    fn assert_equivalent(document: &Document, selectors: &[&str]) {
        let index = SelectorIndex::new(selectors);
        let expected = super::super::select(document.root(), &selectors.join(","));
        let actual = index.select(document.root());
        assert_eq!(
            actual.iter().map(|n| n.id).collect::<Vec<_>>(),
            expected.iter().map(|n| n.id).collect::<Vec<_>>(),
            "{selectors:?}"
        );
    }

    #[test]
    fn fixed_index_matches_full_list_on_every_real_input() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        for entry in manifest["cases"].as_array().unwrap() {
            let case: serde_json::Value = serde_json::from_slice(
                &std::fs::read(directory.join(entry["file"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_equivalent(
                &Document::from(case["inputHtml"].as_str().unwrap()),
                crate::constants::EXACT_SELECTORS,
            );
        }
    }

    #[test]
    fn unusual_css_uses_conservative_fallback() {
        let document = Document::from(
            r#"<div id="a" class="café ad foo"><span class="x bar" data-test="a] b" lang="en-US">Text</span><p class="bar">P</p></div><svg><a class="x"/></svg>"#,
        );
        for selector in [
            ".café",
            ".caf\\e9",
            "div\t.x",
            "div\n.x",
            "div\r.x",
            "div\u{000c}.x",
            "div > .x",
            "div + p",
            "div ~ p",
            ".foo, .bar",
            ":not(a, p)",
            "div:not(.absent)",
            "div:has(span)",
            "[data-test=\"a] b\"]",
            "[lang|=en]",
            "[class~=bar]",
            "[data-test*=\"]\"]",
            "*|a",
            "svg|a",
            "[xml|lang]",
            "[CLASS]",
            "DIV",
            ".foo/* comment */ .bar",
            ".ad:not([class*=gradient])",
        ] {
            if Matcher::new(selector).is_ok() {
                assert_equivalent(&document, &[selector]);
            }
        }
        assert_equivalent(&document, &[".foo", "[lang|=en]", "span.x", ".café"]);

        // The index uses exact class/id keys. dom_query currently matches in
        // NoQuirks mode even without a doctype; an upgrade that changes this
        // must update the index rather than silently drop real matches.
        let quirks = dom_query::Document::from(r#"<div class="AD" id="X">Text</div>"#);
        for selector in [".ad", "#x"] {
            assert!(quirks.select(selector).is_empty());
            assert_equivalent(&quirks, &[selector]);
        }
    }
}
