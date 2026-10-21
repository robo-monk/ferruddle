//! Normalize syntax highlighters into text-only pre/code pairs.
use crate::dom;
use crate::utils::tag;
use dom_query::NodeRef;

const LANGUAGES: &str = " abap actionscript ada adoc agda antlr4 applescript arduino armasm asciidoc aspnet atom bash batch c clojure cmake cobol coffeescript cpp c++ crystal csharp cs css dart django dockerfile dotnet elixir elm erlang fish fortran fsharp gdscript gitignore glsl go golang gradle graphql groovy haskell hs haxe hlsl html idris java javascript js jsx jsdoc json jsonp julia kotlin latex lean lean4 lisp elisp livescript lua makefile markdown md markup masm mathml matlab mongodb mysql nasm nginx nim nix objc ocaml pascal perl php plaintext text postgresql powershell prolog puppet python regex rss ruby rb rust scala scheme shell sh solidity sparql sql ssml svg swift tcl terraform tex toml typescript ts tsx unrealscript verilog vhdl webassembly wasm xml yaml yml zig ";

pub(crate) fn standardize(root: NodeRef<'_>) {
    let candidates: Vec<_> = dom::elements(root)
        .into_iter()
        .filter(|node| candidate(*node))
        .collect();
    for node in candidates {
        if !dom::same_node(node, root) && !node.ancestors_it(None).any(|n| dom::same_node(n, root))
        {
            continue;
        }
        // A source wrapper may enclose several independent examples. Its pre
        // children will be processed separately instead of collapsing examples.
        let pres = dom::select(node, "pre");
        if tag(node) != "pre" && pres.len() > 1 && !crate::dom::exists(node, "table") {
            continue;
        }
        let target = extraction_target(node);
        let github_lines = dom::select(node, "td.blob-code");
        let mut language = language(node);
        if language.is_empty() {
            let annotated = dom::select(
                node,
                "code[data-lang],code[data-language],code[language],code[class*=language-]",
            )
            .into_iter()
            .next();
            if let Some(code) = annotated {
                language = self::language(code);
            }
        }
        if language.is_empty() {
            for ancestor in node.ancestors_it(Some(5)) {
                language = self::language(ancestor);
                if !language.is_empty() {
                    break;
                }
            }
        }
        if language.is_empty()
            && let Some(cm) = dom::select(node, ".cm-content").into_iter().next()
        {
            for label in dom::select(node, "div,span") {
                if dom::same_node(label, cm)
                    || cm.ancestors_it(None).any(|a| dom::same_node(a, label))
                {
                    continue;
                }
                let text = structured_text(label, 0).trim().to_ascii_lowercase();
                if known_language(&text) {
                    language = text;
                    break;
                }
            }
        }
        clean_chrome(node);
        let text = if github_lines.is_empty() {
            structured_text(target, 0)
        } else {
            github_lines
                .into_iter()
                .map(|line| structured_text(line, 0).trim_end_matches('\n').to_owned())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let verso =
            node.has_class("hl") && (node.has_class("block") || node.has_class("lean-output"));
        let dedent = !matches!(tag(node).as_str(), "pre" | "code") || text.starts_with('\n');
        let text = clean_code(&text, dedent, verso);
        remove_sibling_chrome(node);
        dom::retag(node, "pre");
        node.remove_all_attrs();
        if verso {
            node.set_attr("data-verso-code", "true");
        }
        node.remove_children();
        let code = node.tree.new_element("code");
        if !language.is_empty() && language != "plaintext" && language != "text" {
            code.set_attr("data-lang", &language);
            code.set_attr("class", &format!("language-{language}"));
        }
        dom::append_text(code, &text);
        node.append_child(&code);
        // Invalid code>pre nesting must not turn a block back into inline code.
        if let Some(parent) = node.parent().filter(|n| tag(*n) == "code") {
            dom::unwrap(parent);
        }
    }
    merge_verso(root);
}

fn candidate(node: NodeRef<'_>) -> bool {
    let name = tag(node);
    if name == "pre" || (name == "code" && node.has_class("hl") && node.has_class("block")) {
        return true;
    }
    let classes = node.class().unwrap_or_default().to_ascii_lowercase();
    let wrapper = node.has_class("highlight")
        || node.has_class("syntaxhighlighter")
        || node.has_class("highlight-source")
        || node.has_class("wp-block-syntaxhighlighter-code")
        || node.has_class("wp-block-code")
        || classes.contains("prismjs")
        || (name == "div"
            && classes
                .split_whitespace()
                .any(|class| class.starts_with("language-")))
        || (node.has_class("code-block")
            && (node.has_attr("data-lang") || node.has_attr("language")));
    wrapper
        && (crate::dom::exists(node, "pre,code,.line,.cm-content,td.blob-code")
            || node.has_class("code-block"))
}

fn extraction_target(node: NodeRef<'_>) -> NodeRef<'_> {
    if let Some(cm) = dom::select(node, ".cm-content").into_iter().next() {
        return cm;
    }
    if let Some(code) = dom::select(node, ".syntaxhighlighter .code .container,.code .container,.rouge-code pre,td.code pre,td.code .container").into_iter().next() { return code; }
    if tag(node) == "pre" || tag(node) == "code" {
        return node;
    }
    let pres = dom::select(node, "pre");
    pres.iter().copied().find(|pre| crate::dom::exists(*pre, "code[data-lang],code[data-language],code[language],code[class*=language-],.line,[data-line]") && !is_gutter(*pre))
        .or_else(|| pres.iter().copied().find(|pre| !is_gutter(*pre)))
        .unwrap_or(node)
}

fn language(node: NodeRef<'_>) -> String {
    for attribute in ["data-lang", "data-language", "language"] {
        if let Some(value) = node.attr(attribute).filter(|v| !v.trim().is_empty()) {
            return safe_language(&value);
        }
    }
    let classes = node.class().unwrap_or_default();
    for class in classes.split_whitespace() {
        for prefix in [
            "language-",
            "lang-",
            "brush-",
            "syntax-",
            "code-",
            "code-snippet__",
            "highlight-",
        ] {
            if let Some(value) = class.strip_prefix(prefix)
                && !value.is_empty()
                && (matches!(prefix, "language-" | "lang-") || known_language(value))
            {
                return safe_language(value);
            }
        }
        for suffix in ["-code", "-snippet"] {
            if let Some(value) = class.strip_suffix(suffix)
                && known_language(value)
            {
                return value.to_lowercase();
            }
        }
    }
    classes
        .split_whitespace()
        .find(|class| known_language(class))
        .map(str::to_lowercase)
        .unwrap_or_default()
}

fn safe_language(value: &str) -> String {
    value
        .split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '.' | '#'))
        .collect::<String>()
        .to_ascii_lowercase()
}
fn known_language(value: &str) -> bool {
    !value.is_empty()
        && LANGUAGES
            .split_whitespace()
            .any(|language| language.eq_ignore_ascii_case(value))
}

fn is_gutter(node: NodeRef<'_>) -> bool {
    let class = node.class().unwrap_or_default();
    class.split_whitespace().any(|class| {
        matches!(
            class,
            "lnt"
                | "lineno"
                | "linenumber"
                | "react-syntax-highlighter-line-number"
                | "rouge-gutter"
                | "gutter"
                | "line-number"
        ) || class.ends_with("-line-number")
    }) || node
        .ancestors_it(None)
        .any(|ancestor| ancestor.has_class("rouge-gutter") || ancestor.has_class("gutter"))
}
fn line(node: NodeRef<'_>) -> bool {
    matches!(tag(node).as_str(), "div" | "span")
        && (node.has_attr("data-line")
            || node.has_attr("data-line-number")
            || node.class().is_some_and(|class| {
                class.split_whitespace().any(|class| {
                    matches!(class, "line" | "ec-line" | "cm-line" | "code-line")
                        || class.starts_with("line-")
                })
            }))
}

fn clean_chrome(root: NodeRef<'_>) {
    for node in dom::select(
        root,
        "button,style,[class*=codeblock-button],.hover-info,.hover-container",
    ) {
        node.remove_from_parent();
    }
    for node in dom::select(root, "div,span") {
        let class = node.class().unwrap_or_default().to_ascii_lowercase();
        if ["header", "toolbar", "titlebar", "title-bar"]
            .iter()
            .any(|word| class.contains(word))
            && node.text().split_whitespace().count() <= 5
            && !node
                .ancestors_it(None)
                .take_while(|ancestor| !dom::same_node(*ancestor, root))
                .any(line)
            && !crate::dom::exists(node, "pre,.line,[data-line]")
        {
            node.remove_from_parent();
        }
    }
}

fn structured_text(node: NodeRef<'_>, depth: usize) -> String {
    if depth > 512 {
        return node.text().to_string();
    }
    if node.is_text() {
        let text = node.text();
        if text.trim().is_empty()
            && node
                .parent()
                .is_some_and(|parent| parent.children_it(false).any(line))
        {
            return String::new();
        }
        return text.to_string();
    }
    let name = tag(node);
    if is_gutter(node)
        || matches!(name.as_str(), "button" | "style" | "script")
        || node.has_class("hover-info")
        || node.has_class("hover-container")
    {
        return String::new();
    }
    if name == "br" {
        return if node.prev_element_sibling().is_some_and(line) {
            String::new()
        } else {
            "\n".to_owned()
        };
    }
    let children = node.element_children();
    if matches!(name.as_str(), "div" | "span") && children.len() == 2 {
        let gutter = children[0].text();
        if !gutter.trim().is_empty() && gutter.trim().chars().all(|c| c.is_ascii_digit()) {
            return format!(
                "{}\n",
                structured_text(children[1], depth + 1).trim_end_matches('\n')
            );
        }
    }
    let mut output = String::new();
    for child in node.children_it(false) {
        output.push_str(&structured_text(child, depth + 1));
    }
    if line(node) {
        output.truncate(output.trim_end_matches('\n').len());
        output.push('\n');
    }
    output
}

fn clean_code(value: &str, dedent: bool, verso: bool) -> String {
    let value = value
        .replace('\t', "    ")
        .replace('\u{a0}', " ")
        .replace("\r\n", "\n");
    if verso && value.trim().is_empty() {
        return "\n".repeat(value.bytes().filter(|byte| *byte == b'\n').count().max(1));
    }
    // Verso source fragments use trailing newlines as authored section gaps.
    // Whitespace after their last newline belongs to the HTML template.
    let mut value = if verso {
        value
            .trim_end_matches([' ', '\t'])
            .trim_start_matches('\n')
            .to_owned()
    } else {
        value.trim_matches('\n').to_owned()
    };
    if dedent {
        let baseline = value
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.bytes().take_while(|byte| *byte == b' ').count())
            .min()
            .unwrap_or(0);
        if baseline > 0 {
            value = value
                .split('\n')
                .map(|line| &line[baseline.min(line.len() - line.trim_start_matches(' ').len())..])
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    // A highlighter's display lines already carry their authored empty lines.
    value
}

fn remove_sibling_chrome(node: NodeRef<'_>) {
    let mut ancestor = node;
    for _ in 0..3 {
        let Some(container) = ancestor.parent() else {
            break;
        };
        if tag(container) == "body"
            || container.element_children().len() > 5
            || dom::closest(container, "[data-callout]").is_some()
        {
            break;
        }
        for sibling in container.element_children() {
            if dom::same_node(sibling, ancestor)
                || !matches!(tag(sibling).as_str(), "div" | "span")
                || !dom::select(
                    sibling,
                    "pre,code,img,svg,table,h1,h2,h3,h4,h5,h6,p,blockquote,ul,ol,hr",
                )
                .is_empty()
            {
                continue;
            }
            let text = structured_text(sibling, 0).trim().to_ascii_lowercase();
            let class = sibling.class().unwrap_or_default().to_ascii_lowercase();
            if text.split_whitespace().count() <= 5
                && (["header", "toolbar", "titlebar", "filename"]
                    .iter()
                    .any(|word| class.contains(word))
                    || known_language(&text)
                    || matches!(text.as_str(), "copy" | "run" | "command line"))
            {
                sibling.remove_from_parent();
            }
        }
        ancestor = container;
    }
}

fn merge_verso(root: NodeRef<'_>) {
    for node in dom::select(root, "pre[data-verso-code]") {
        if node.parent().is_none() {
            continue;
        }
        loop {
            let mut next = node.next_sibling();
            let mut whitespace = Vec::new();
            while let Some(sibling) = next {
                if sibling.is_text() && sibling.text().trim().is_empty() {
                    whitespace.push(sibling);
                    next = sibling.next_sibling();
                } else {
                    break;
                }
            }
            let Some(next) =
                next.filter(|next| tag(*next) == "pre" && next.has_attr("data-verso-code"))
            else {
                break;
            };
            let Some(code) = node.first_element_child() else {
                break;
            };
            let Some(other) = next.first_element_child() else {
                break;
            };
            if code.attr("data-lang") != other.attr("data-lang") {
                break;
            }
            let mut combined = code.text().to_string();
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&other.text());
            code.set_text(combined.as_str());
            next.remove_from_parent();
            for blank in whitespace {
                blank.remove_from_parent();
            }
        }
    }
}
