/// Count words with Defuddle's BMP CJK and ASCII whitespace rules.
pub fn count_words(text: &str) -> usize {
    let mut counter = WordCounter::default();
    for ch in text.chars() {
        counter.push(ch);
    }
    counter.count
}

#[derive(Default)]
struct WordCounter {
    count: usize,
    in_word: bool,
}

impl WordCounter {
    fn push(&mut self, ch: char) {
        let cjk = matches!(ch as u32,
            0x3040..=0x309f | 0x30a0..=0x30ff | 0x3400..=0x4dbf |
            0x4e00..=0x9fff | 0xf900..=0xfaff | 0xac00..=0xd7af);
        if cjk {
            self.count += 1;
            self.in_word = false;
        } else if ch <= ' ' {
            self.in_word = false;
        } else if !self.in_word {
            self.count += 1;
            self.in_word = true;
        }
    }
}

/// Count the same concatenated text as `count_words(&node.text())` without
/// building it. Adjacent text nodes share word state.
pub fn text_words(node: dom_query::NodeRef<'_>) -> usize {
    let mut counter = WordCounter::default();
    for current in std::iter::once(node).chain(node.descendants_it()) {
        current.query_or((), |node| {
            if let dom_query::NodeData::Text { contents } = &node.data {
                contents.chars().for_each(|ch| counter.push(ch));
            }
        });
    }
    counter.count
}

/// Test the same concatenated text as `count_words(&node.text())`, stopping
/// as soon as the limit is exceeded. Inline text nodes share word state.
pub(crate) fn words_at_most(node: dom_query::NodeRef<'_>, limit: usize) -> bool {
    let mut counter = WordCounter::default();
    for current in std::iter::once(node).chain(node.descendants_it()) {
        let exceeded = current.query_or(false, |node| {
            let dom_query::NodeData::Text { contents } = &node.data else {
                return false;
            };
            for ch in contents.chars() {
                counter.push(ch);
                if counter.count > limit {
                    return true;
                }
            }
            false
        });
        if exceeded {
            return false;
        }
    }
    true
}

/// Count visible words with separators between structural blocks. Inline
/// wrappers do not split a word (e.g. `<em>re</em>tain` remains one word).
pub fn count_content_words(root: dom_query::NodeRef<'_>) -> usize {
    let mut text = String::new();
    let mut pending = vec![(root, false)];
    while let Some((node, exit)) = pending.pop() {
        if node.is_text() {
            text.push_str(&node.text());
            continue;
        }
        let name = tag(node);
        let block = crate::constants::BLOCK_LEVEL_ELEMENTS.contains(&name.as_str())
            || matches!(name.as_str(), "br" | "td" | "th" | "dt" | "dd");
        if block {
            text.push(' ')
        }
        if !exit {
            if block {
                pending.push((node, true))
            }
            for child in node.children().into_iter().rev() {
                pending.push((child, false))
            }
        }
    }
    count_words(&text)
}

pub fn normalize_text(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .replace(['\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'], "'")
        .replace(['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}'], "-")
        .replace(['\u{201c}', '\u{201d}', '\u{201e}', '\u{201f}'], "\"")
        .replace('\u{2026}', "...")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// An element's ASCII-lowercase local name, or empty for other nodes.
pub fn tag(node: dom_query::NodeRef<'_>) -> Tag {
    match node.qual_name_ref() {
        Some(name) if name.local.bytes().any(|b| b.is_ascii_uppercase()) => {
            Tag::Lowered(name.local.as_ref().to_ascii_lowercase())
        }
        Some(name) => Tag::Interned(name.local.clone()),
        None => Tag::Lowered(String::new()),
    }
}

/// A lowercase tag name that reuses the parser's interned name. Only names with
/// ASCII uppercase, such as SVG `foreignObject`, are copied.
#[derive(Clone, Debug)]
pub enum Tag {
    Interned(dom_query::LocalName),
    Lowered(String),
}

impl Tag {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Interned(name) => name,
            Self::Lowered(name) => name,
        }
    }
}

impl std::ops::Deref for Tag {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for Tag {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Tag {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Tag> for &str {
    fn eq(&self, other: &Tag) -> bool {
        *self == other.as_str()
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn attribute(node: dom_query::NodeRef<'_>, name: &str) -> String {
    node.attr(name)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

pub fn contains(ancestor: dom_query::NodeRef<'_>, node: dom_query::NodeRef<'_>) -> bool {
    std::ptr::eq(ancestor.tree, node.tree)
        && (ancestor.id == node.id
            || node
                .ancestors_it(None)
                .any(|parent| parent.id == ancestor.id))
}

pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

pub fn parse_number(value: &str) -> f64 {
    let prefix: String = value
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-'))
        .collect();
    prefix.parse().unwrap_or(0.0)
}

pub fn matches(node: dom_query::NodeRef<'_>, selector: &str) -> bool {
    crate::dom::matcher(selector).is_some_and(|matcher| node.is_match(&matcher))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom_query::Document;

    #[test]
    fn tag_equals_lowercased_local_name_on_every_real_input() {
        let unusual = Document::from(concat!(
            "<svg><foreignObject><DIV>x</DIV></foreignObject><linearGradient/>",
            "<clipPath/></svg><math><mi>x</mi></math><My-Widget>x</My-Widget>",
            "<template><Span>x</Span></template><!--comment-->",
        ));
        for doc in crate::tests::upstream_documents()
            .into_iter()
            .chain([unusual])
        {
            for node in crate::dom::subtree(doc.root()) {
                let expected = node
                    .qual_name_ref()
                    .map(|name| name.local.as_ref().to_ascii_lowercase())
                    .unwrap_or_default();
                assert_eq!(tag(node).as_str(), expected);
            }
        }
    }

    #[test]
    fn text_words_equal_counting_concatenated_text_on_every_real_input() {
        for doc in crate::tests::upstream_documents() {
            for node in crate::dom::subtree(doc.root()) {
                assert_eq!(text_words(node), count_words(&node.text()));
            }
        }
    }

    #[test]
    fn bounded_word_count_agrees_across_text_node_boundaries() {
        let document = Document::from(
            "<article><p>un<b>be</b>lievable re<i>use</i></p><p>你好 world한글</p>\
             <div>one\t<span>two\nthree</span>\u{a0}four<br>five</div>\
             <div>𐐀é\u{2003}x<code>z</code><span>。</span></div></article>",
        );
        for node in std::iter::once(document.root()).chain(document.root().descendants_it()) {
            let expected = count_words(&node.text());
            assert_eq!(text_words(node), expected, "{node:?}");
            for limit in 0..=expected + 1 {
                assert_eq!(
                    words_at_most(node, limit),
                    expected <= limit,
                    "{node:?}, {limit}"
                );
            }
        }
    }
}
