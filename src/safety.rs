//! Unconditional HTML output sanitization, independent of extraction options.

use dom_query::{NodeData, NodeRef};

use crate::dom;

/// Reject script URLs and embedded documents after ignoring whitespace and
/// control characters. Inline image data is allowed only when the caller opts
/// in; an iframe must never opt in, including for `data:image/svg+xml`.
pub fn is_dangerous_url(value: &str, allow_inline_image: bool) -> bool {
    // All tested prefixes fit within 32 characters. Bounding the normalized
    // prefix avoids allocating an entire potentially huge base64 image.
    let normalized: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .take(32)
        .flat_map(char::to_lowercase)
        .collect();
    normalized.starts_with("javascript:")
        || normalized.starts_with("vbscript:")
        || normalized.starts_with("blob:")
        || (normalized.starts_with("data:")
            && !(allow_inline_image && normalized.starts_with("data:image/")))
}

/// An unsafe root must be omitted by the caller. Math scripts are inert only
/// in the HTML namespace; SVG scripts are never exempted by a math MIME type.
pub fn is_unsafe_element(node: NodeRef<'_>) -> bool {
    let Some(name) = node.node_name() else {
        return false;
    };
    let tag = name.to_ascii_lowercase();
    if tag == "script" {
        return !dom::is_html_element(node, "script")
            || !node
                .attr_or("type", "")
                .to_ascii_lowercase()
                .starts_with("math/");
    }
    matches!(
        tag.as_str(),
        "style"
            | "meta"
            | "link"
            | "noscript"
            | "template"
            | "frame"
            | "frameset"
            | "object"
            | "embed"
            | "applet"
            | "base"
            | "animate"
            | "set"
            | "animatemotion"
            | "animatetransform"
            | "animatecolor"
            | "discard"
    )
}

/// Sanitize a selected root and every descendant, including inert template
/// content and foreign namespaces. Returns true when the root itself is unsafe;
/// the root is neutralized and its caller must omit it from output entirely.
pub fn sanitize(root: NodeRef<'_>) -> bool {
    let unsafe_root = is_unsafe_element(root);
    // Snapshot before removals: template fragments are outside ordinary DOM
    // traversal, and detached descendants must not retain active payloads.
    let nodes = dom::subtree(root);
    for node in nodes.iter().rev().copied() {
        if is_unsafe_element(node) {
            node.remove_all_attrs();
            node.remove_children();
            if let Some(content) = dom::template_content(node) {
                content.remove_children();
            }
            if !dom::same_node(node, root) {
                node.remove_from_parent();
            }
            continue;
        }
        if !node.is_element() {
            // Suspense comments have served their purpose before this pass.
            // Removing comments and PIs also avoids mutation of malformed markup
            // when a consumer reparses the returned fragment.
            if node.is_comment()
                || node.query_or(false, |node| {
                    matches!(node.data, NodeData::ProcessingInstruction { .. })
                })
            {
                node.remove_from_parent();
            }
            continue;
        }
        let iframe = node
            .node_name()
            .is_some_and(|tag| tag.eq_ignore_ascii_case("iframe"));
        node.update(|tree_node| {
            let NodeData::Element(element) = &mut tree_node.data else {
                return;
            };
            element.attrs.retain_mut(|attribute| {
                // html5ever gives xlink:href local name "href" with an xlink
                // namespace; checking only the serialized attribute name misses it.
                let name = attribute.name.local.to_ascii_lowercase();
                let local = name.rsplit(':').next().unwrap_or(&name);
                if local.starts_with("on") || local == "srcdoc" {
                    return false;
                }
                if matches!(
                    local,
                    "href"
                        | "src"
                        | "action"
                        | "formaction"
                        | "poster"
                        | "background"
                        | "cite"
                        | "longdesc"
                        | "manifest"
                        | "data"
                        | "codebase"
                        | "profile"
                ) {
                    return !is_dangerous_url(&attribute.value, !(iframe && local == "src"));
                }
                if matches!(local, "srcset" | "imagesrcset") {
                    let candidates = dom::srcset_candidates(&attribute.value);
                    let safe: Vec<_> = candidates
                        .iter()
                        .filter(|(url, descriptor)| {
                            !is_dangerous_url(&format!("{url}{descriptor}"), true)
                        })
                        .collect();
                    if safe.len() == candidates.len() {
                        return true;
                    }
                    if safe.is_empty() {
                        return false;
                    }
                    let filtered = safe
                        .into_iter()
                        .map(|(url, descriptor)| {
                            if descriptor.is_empty() {
                                url.clone()
                            } else {
                                format!("{url} {descriptor}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    attribute.value = filtered.as_str().into();
                    return true;
                }
                true
            });
        });
    }
    unsafe_root
}
