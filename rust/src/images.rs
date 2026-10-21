//! Promote explicit lazy image sources and normalize pictures/captions.
use crate::utils::tag;
use crate::{dom, safety};
use dom_query::NodeRef;

pub(crate) fn standardize(root: NodeRef<'_>) {
    for image in dom::elements(root)
        .into_iter()
        .filter(|n| matches!(tag(*n).as_str(), "img" | "source"))
    {
        promote_lazy(image);
    }
    for picture in dom::select(root, "picture") {
        picture_sources(picture);
    }
    for custom in dom::select(root, "uni-image-full-width") {
        if let Some(image) = dom::select(custom, "img").into_iter().next() {
            if let Some(loading) = image.attr("data-loading")
                && let Ok(data) = serde_json::from_str::<serde_json::Value>(&loading)
                && let Some(desktop) = data["desktop"].as_str().filter(|src| usable(src))
            {
                image.set_attr("src", desktop);
            }
            if image.attr("alt").is_none_or(|alt| alt.trim().is_empty())
                && let Some(alt) = custom.attr("alt-text")
            {
                image.set_attr("alt", &alt);
            }
            dom::retag(custom, "figure");
        }
    }
    for image in dom::select(root, "img") {
        let source = image.attr_or("src", "");
        let responsive = image.attr("srcset").is_some_and(|srcset| {
            dom::srcset_candidates(&srcset)
                .iter()
                .any(|(src, _)| usable(src) && !is_base64_placeholder(src))
        });
        if (source.trim().is_empty() || is_base64_placeholder(&source)) && responsive {
            if let Some((source, _)) = image.attr("srcset").and_then(|srcset| {
                dom::srcset_candidates(&srcset)
                    .into_iter()
                    .find(|(src, _)| usable(src) && !is_base64_placeholder(src))
            }) {
                image.set_attr("src", &source);
            }
        } else if is_base64_placeholder(&source) && !responsive {
            image.remove_from_parent();
        }
    }
    // Only narrow image wrappers are eligible. Article figures with paragraphs
    // and galleries retain every image and all prose.
    for container in dom::select(root, "figure,span,p") {
        let name = tag(container);
        let images = dom::select(container, "img,video");
        if images.len() != 1 || container.parent().is_none() {
            continue;
        }
        if name == "span"
            && container.element_children().iter().any(|n| {
                matches!(
                    tag(*n).as_str(),
                    "p" | "div"
                        | "section"
                        | "article"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "ul"
                        | "ol"
                        | "table"
                        | "pre"
                )
            })
        {
            continue;
        }
        let existing = dom::select(container, "figcaption").into_iter().next();
        if name == "figure"
            && dom::select(container, "p").iter().any(|p| {
                !p.ancestors_it(None)
                    .take_while(|n| !dom::same_node(*n, container))
                    .any(|n| tag(n) == "figcaption")
            })
        {
            continue;
        }
        let caption = existing.or_else(|| {
            dom::select(
                container,
                "[class*=caption],[class*=credit],[class*=description]",
            )
            .into_iter()
            .find(|n| meaningful_caption(&n.text()))
        });
        let caption = caption.or_else(|| {
            if name != "span" && name != "p" {
                return None;
            }
            container.element_children().into_iter().find(|n| {
                matches!(tag(*n).as_str(), "em" | "i" | "small" | "cite" | "span")
                    && !crate::dom::exists(*n, "img,video")
                    && meaningful_caption(&n.text())
            })
        });
        let caption = caption.or_else(|| {
            if !matches!(name.as_str(), "span" | "figure")
                || !container.text().trim().is_empty()
                || container
                    .ancestors_it(None)
                    .any(|ancestor| tag(ancestor) == "figure")
            {
                return None;
            }
            let alt = images[0]
                .attr("alt")
                .filter(|alt| meaningful_caption(alt))?;
            let caption = container.tree.new_element("figcaption");
            dom::append_text(caption, &alt);
            container.append_child(&caption);
            Some(caption)
        });
        if let Some(caption) = caption
            && (existing.is_some() || meaningful_caption(&caption.text()))
        {
            dom::retag(container, "figure");
            dom::retag(caption, "figcaption");
            // Keep caption text and inline links/formatting through node moves.
            // Flatten only wrappers that contain exactly this caption.
            if let Some(parent) = caption.parent().filter(|parent| {
                !dom::same_node(*parent, container)
                    && parent.element_children().len() == 1
                    && parent.text().trim() == caption.text().trim()
            }) {
                parent.insert_before(&caption);
                parent.remove_from_parent();
            }
        }
    }
}

fn usable(value: &str) -> bool {
    !value.trim().is_empty() && !safety::is_dangerous_url(value, true)
}

pub(crate) fn is_base64_placeholder(source: &str) -> bool {
    let source = source.trim_start();
    let lower = source.to_ascii_lowercase();
    lower.starts_with("data:image/")
        && !lower.starts_with("data:image/svg+xml")
        && lower
            .find(";base64,")
            .is_some_and(|start| source.len() - start - ";base64,".len() < 133)
}

fn svg_inline(source: &str) -> bool {
    source
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("data:image/svg+xml")
}

fn promote_lazy(image: NodeRef<'_>) {
    for (target, sources) in [
        ("src", &["data-src", "data-original", "data-lazy-src"][..]),
        ("srcset", &["data-srcset", "data-lazy-srcset"][..]),
    ] {
        let mut value = None;
        for source in sources {
            if value.is_none()
                && let Some(candidate) = image.attr(source).filter(|value| !value.trim().is_empty())
            {
                if target == "srcset" {
                    let candidates: Vec<_> = dom::srcset_candidates(&candidate)
                        .into_iter()
                        .filter(|(url, _)| usable(url))
                        .collect();
                    if !candidates.is_empty() {
                        value = Some(format_srcset(&candidates));
                    }
                } else if usable(&candidate) {
                    value = Some(candidate.trim().to_owned());
                }
            }
            image.remove_attr(source);
        }
        if let Some(value) = value {
            image.set_attr(target, &value);
        }
    }
    if let Some(loader) = image
        .attr("data-image-loader")
        .filter(|value| usable(value))
    {
        let source = image.attr_or("src", "");
        if source.is_empty() || is_base64_placeholder(&source) || svg_inline(&source) {
            image.set_attr("src", loader.trim());
        }
    }
    for attribute in ["data-image-loader", "data-ll-status", "loading"] {
        image.remove_attr(attribute);
    }
    image.remove_class("lazy");
    image.remove_class("lazyload");
}

fn format_srcset(candidates: &[(String, String)]) -> String {
    candidates
        .iter()
        .map(|(url, descriptor)| {
            if descriptor.is_empty() {
                url.clone()
            } else {
                format!("{url} {descriptor}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn picture_sources(picture: NodeRef<'_>) {
    let sources = dom::select(picture, "source");
    let candidates: Vec<_> = sources
        .iter()
        .copied()
        .filter_map(|source| {
            let source_set = source.attr("srcset")?;
            let candidates: Vec<_> = dom::srcset_candidates(&source_set)
                .into_iter()
                .filter(|(url, _)| usable(url))
                .collect();
            if candidates.is_empty() {
                None
            } else {
                Some((source, candidates))
            }
        })
        .collect();
    let best = candidates
        .iter()
        .find(|(source, _)| !source.has_attr("media"))
        .or_else(|| {
            candidates.iter().fold(None, |best, candidate| {
                if best.is_none_or(|(_, current): &(NodeRef<'_>, Vec<(String, String)>)| {
                    resolution(&candidate.1) > resolution(current)
                }) {
                    Some(candidate)
                } else {
                    best
                }
            })
        });
    let image = dom::select(picture, "img").into_iter().next();
    if let Some((_, candidates)) = best {
        let image = image.unwrap_or_else(|| {
            let image = picture.tree.new_element("img");
            picture.append_child(&image);
            image
        });
        image.set_attr("srcset", &format_srcset(candidates));
        if let Some((url, _)) = candidates
            .iter()
            .find(|(src, _)| !is_base64_placeholder(src) && !svg_inline(src))
            .or_else(|| candidates.first())
        {
            image.set_attr("src", url);
        }
    }
    for source in sources {
        source.remove_from_parent();
    }
}

fn resolution(candidates: &[(String, String)]) -> f64 {
    candidates
        .iter()
        .map(|(_, descriptor)| {
            descriptor
                .strip_suffix('w')
                .or_else(|| descriptor.strip_suffix('x'))
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(1.0)
        })
        .fold(0.0, f64::max)
}

fn meaningful_caption(value: &str) -> bool {
    let value = value.trim();
    value.chars().count() >= 10
        && !value.starts_with("http://")
        && !value.starts_with("https://")
        && !value.chars().all(|c| c.is_ascii_digit() || c == '-')
        && !(value.split_whitespace().count() == 1
            && [".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg"]
                .iter()
                .any(|suffix| value.to_ascii_lowercase().ends_with(suffix)))
}
