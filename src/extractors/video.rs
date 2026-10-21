use super::*;
use serde_json::Value;
fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
pub(super) fn youtube(doc: &Document, url: &Url, metadata: &Metadata) -> Extracted {
    let id = if url.host_str() == Some("youtu.be") {
        url.path().trim_start_matches('/').to_string()
    } else if let Some((_, tail)) = url.path().split_once("/shorts/") {
        tail.split('/').next().unwrap_or_default().into()
    } else {
        url.query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let mut video = Value::Null;

    for script in nodes(doc.root(), "script[type='application/ld+json']") {
        let Ok(data) = serde_json::from_str::<Value>(&script.text()) else {
            continue;
        };
        let items = if let Value::Array(a) = data {
            a
        } else {
            vec![data]
        };
        if let Some(v) = items.into_iter().find(|v| {
            s(v, "@type") == "VideoObject"
                && (id.is_empty()
                    || fallback(s(v, "@id"), &fallback(s(v, "url"), &s(v, "embedUrl")))
                        .contains(&id))
        }) {
            if !s(&v, "description").is_empty() {
                video = v;

                break;
            }
            if v.get("comment").is_some() || video.is_null() {
                video = v;
            }
        }
    }
    if video.is_null()
        && !id.is_empty()
        && a(q(doc.root(), "meta[property='og:url']"), "content").contains(&id)
    {
        video = serde_json::json!({"name":a(q(doc.root(),"meta[property='og:title']"),"content"),"description":a(q(doc.root(),"meta[property='og:description']"),"content"),"thumbnailUrl":a(q(doc.root(),"meta[property='og:image']"),"content")});
    }

    let mut author = String::new();
    for selector in [
        "ytd-video-owner-renderer #channel-name a[href^='/@']",
        "#owner-name a[href^='/@']",
    ] {
        author = t(q(doc.root(), selector));
        if !author.is_empty() {
            break;
        }
    }
    if author.is_empty()
        && let Some(root) = q(doc.root(), "[itemprop=author]")
    {
        author = fallback(
            a(q(root, "meta[itemprop=name]"), "content").trim().into(),
            &fallback(
                a(q(root, "link[itemprop=name]"), "content").trim().into(),
                &t(q(root, "[itemprop=name],a,span")),
            ),
        );
    }
    let player = inline_json(doc, "ytInitialPlayerResponse").filter(|v| {
        !id.is_empty()
            && (v.pointer("/videoDetails/videoId").and_then(Value::as_str) == Some(&id)
                || v.pointer("/microformat/playerMicroformatRenderer/externalVideoId")
                    .and_then(Value::as_str)
                    == Some(&id))
    });
    if author.is_empty()
        && let Some(player) = &player
    {
        author = player
            .pointer("/videoDetails/author")
            .or_else(|| player.pointer("/videoDetails/ownerChannelName"))
            .or_else(|| player.pointer("/microformat/playerMicroformatRenderer/ownerChannelName"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into();
    }
    if author.is_empty() {
        author = s(&video, "author");
    }
    let description = s(&video, "description");
    let mut html = format!(
        "<iframe width=\"560\" height=\"315\" src=\"https://www.youtube.com/embed/{}\" title=\"YouTube video player\" allowfullscreen></iframe><p>{}</p>",
        esc(&id),
        esc(&description).replace('\n', "<br>")
    );
    let segments = dom_segments(doc);
    if !segments.is_empty() {
        html.push_str(&transcript(segments));
    }
    let mut meta = metadata.clone();
    meta.title = s(&video, "name");
    meta.author = author;
    meta.site = "YouTube".into();
    meta.image = video
        .get("thumbnailUrl")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    meta.published = s(&video, "uploadDate");
    meta.description = description
        .chars()
        .take(200)
        .collect::<String>()
        .trim()
        .into();
    if let Some(tracks) = player
        .as_ref()
        .and_then(|p| p.pointer("/captions/playerCaptionsTracklistRenderer/captionTracks"))
        .and_then(Value::as_array)
    {
        let selected = t(q(doc.root(), "#footer #label-text"));
        if let Some(track) = tracks
            .iter()
            .find(|track| {
                track
                    .pointer("/name/simpleText")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s == selected)
            })
            .or_else(|| {
                if tracks.len() == 1 {
                    tracks.first()
                } else {
                    None
                }
            })
        {
            let lang = s(track, "languageCode");
            if !lang.is_empty() {
                meta.language = lang;
            }
        }
    }
    direct("YoutubeExtractor", html, meta)
}
fn inline_json(doc: &Document, name: &str) -> Option<Value> {
    for script in nodes(doc.root(), "script") {
        let raw = script.text();
        let Some(start) = raw
            .find(name)
            .and_then(|start| raw[start..].find('{').map(|i| i + start))
        else {
            continue;
        };
        let mut depth = 0;
        let mut string = false;
        let mut escape = false;
        for (i, c) in raw[start..].char_indices() {
            if string {
                if escape {
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    string = false;
                }
            } else {
                match c {
                    '"' => string = true,
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            if let Ok(v) = serde_json::from_str(&raw[start..start + i + 1]) {
                                return Some(v);
                            }
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}
#[derive(Clone)]
struct Segment {
    start: u64,
    text: String,
    speaker: Option<usize>,
}
fn dom_segments(doc: &Document) -> Vec<Segment> {
    let Some(container)=q(doc.root(),"ytd-engagement-panel-section-list-renderer[target-id=engagement-panel-searchable-transcript] #segments-container").or_else(||q(doc.root(),"ytm-macro-markers-list-renderer .ytm-macro-markers-list-container"))else{return Vec::new()};
    nodes(
        container,
        "ytd-transcript-segment-renderer,transcript-segment-view-model",
    )
    .into_iter()
    .filter_map(|n| {
        let timestamp = t(q(
            n,
            ".segment-timestamp,.ytwTranscriptSegmentViewModelTimestamp",
        ));
        let text = t(q(n, ".segment-text,span.yt-core-attributed-string"));
        if text.is_empty() {
            return None;
        }
        let parts = timestamp
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if parts.len() < 2 || parts.len() > 3 {
            return None;
        }
        let start = parts
            .into_iter()
            .try_fold(0u64, |a, b| a.checked_mul(60)?.checked_add(b))?;
        Some(Segment {
            start,
            text,
            speaker: None,
        })
    })
    .collect()
}
fn sentence_end(s: &str) -> bool {
    re!(r#"[.!?。！？]["'’”)」』）]*\s*$"#).is_match(s)
}
fn sentence_groups(segments: Vec<Segment>) -> Vec<Segment> {
    fn push(pending: &mut Vec<Segment>, out: &mut Vec<Segment>) {
        if pending.is_empty() {
            return;
        }
        let mut seg = pending.remove(0);
        for next in pending.drain(..) {
            seg.text.push(' ');
            seg.text.push_str(&next.text);
        }
        seg.text = seg.text.trim().into();
        if !seg.text.is_empty() {
            out.push(seg);
        }
    }
    let mut out = Vec::new();
    let mut pending: Vec<Segment> = Vec::new();
    for seg in segments {
        if pending
            .last()
            .is_some_and(|last| seg.start.saturating_sub(last.start) > 20)
        {
            push(&mut pending, &mut out);
        }
        let ends = sentence_end(&seg.text);
        pending.push(seg);
        if ends {
            push(&mut pending, &mut out);
            continue;
        }
        if pending
            .last()
            .unwrap()
            .start
            .saturating_sub(pending[0].start)
            >= 30
        {
            let min = pending[0].start.saturating_add(15);
            let mut best = None;
            for i in (0..pending.len()).rev() {
                if pending[i].start < min {
                    break;
                }
                if let Some(c)=re!(r#"^(.*[.!?]["'’”)]*)\s+([A-Z].*)$|^(.*[。！？][」』）]*)([\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}].*)$"#).captures(&pending[i].text){let before=c.get(1).or_else(||c.get(3)).unwrap().as_str().to_string();let after=c.get(2).or_else(||c.get(4)).unwrap().as_str().to_string();let mut next=pending[i].clone();pending[i].text=before;next.text=after;pending.insert(i+1,next);best=Some(i+1);break;}
            }
            if best.is_none() {
                best = (1..pending.len())
                    .filter(|i| pending[*i].start >= min)
                    .max_by_key(|i| (pending[*i].start.saturating_sub(pending[*i - 1].start), *i));
            }
            if let Some(i) = best.filter(|i| *i > 0 && *i < pending.len()) {
                let rest = pending.split_off(i);
                push(&mut pending, &mut out);
                pending = rest;
            } else {
                push(&mut pending, &mut out);
            }
        }
    }
    push(&mut pending, &mut out);
    out
}
fn transcript(segments: Vec<Segment>) -> String {
    let speaker_mode = segments.iter().any(|s| re!(r"^(>>|-\s)").is_match(&s.text));
    let groups = if speaker_mode {
        let mut turns: Vec<Vec<Segment>> = Vec::new();
        let mut current = Vec::new();
        let mut speaker = 1;
        let mut previous = String::new();
        for mut seg in segments {
            let marker = re!(r"^(>>|-\s)").is_match(&seg.text);
            seg.text = re!(r"^(>>\s*|-\s+)").replace(&seg.text, "").into_owned();
            if marker
                && (previous.is_empty() || sentence_end(&previous))
                && !previous.trim_end().ends_with(',')
            {
                if !current.is_empty() {
                    turns.push(std::mem::take(&mut current));
                }
                speaker = (speaker + 1) % 2;
                seg.speaker = Some(speaker);
            } else {
                seg.speaker = current.first().and_then(|s: &Segment| s.speaker);
            }
            previous = seg.text.clone();
            current.push(seg);
        }
        if !current.is_empty() {
            turns.push(current);
        }
        turns.into_iter().flat_map(sentence_groups).collect()
    } else {
        sentence_groups(segments)
    };
    let mut html = String::from("<div class=\"youtube transcript\"><h2>Transcript</h2>");
    for group in groups {
        let s = group.start;
        let timestamp = if s >= 3600 {
            format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
        } else {
            format!("{}:{:02}", s / 60, s % 60)
        };
        html.push_str(&format!("<p class=\"transcript-segment{}\"><strong><span class=\"timestamp\" data-timestamp=\"{s}\">{timestamp}</span></strong> · {}</p>",group.speaker.map(|s|format!(" speaker-{s}")).unwrap_or_default(),esc(&group.text)));
    }
    html.push_str("</div>");
    html
}
pub(super) fn bilibili(doc: &Document, parsed: &Url, metadata: &Metadata) -> Option<Extracted> {
    let capture = re!(r"/video/(BV[0-9A-Za-z]+)/?").captures(parsed.path())?;
    let bvid = &capture[1];
    let page = parsed
        .query_pairs()
        .find(|(k, _)| k == "p")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(1);
    let mut meta = metadata.clone();
    meta.title = t(q(doc.root(), "title"));
    meta.author.clear();
    meta.description.clear();
    meta.image.clear();
    meta.published.clear();
    meta.site = "Bilibili".into();
    Some(direct(
        "BilibiliExtractor",
        format!(
            "<iframe width=\"560\" height=\"315\" src=\"https://player.bilibili.com/player.html?bvid={bvid}&amp;page={page}&amp;high_quality=1&amp;danmaku=0\" title=\"Bilibili video player\" allowfullscreen></iframe>"
        ),
        meta,
    ))
}
