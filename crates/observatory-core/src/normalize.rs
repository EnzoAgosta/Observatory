use unicode_normalization::UnicodeNormalization;

use crate::ir::{ContentNode, LanguageTag, LanguageTagError};

pub enum CollapseMode {
    CollapseAdjacent,
    DropEmpty,
}

pub enum TrimMode {
    TrimOuterLeading,
    TrimOuterTrailing,
    TrimOuterBoth,
    TrimAllLeading,
    TrimAllTrailing,
    TrimAllBoth,
}

pub fn collapse(content: &[ContentNode], mode: CollapseMode) -> Vec<ContentNode> {
    match mode {
        CollapseMode::CollapseAdjacent => collapse_adjacent(content),
        CollapseMode::DropEmpty => drop_empty(content),
    }
}

fn collapse_adjacent(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut collapsed: Vec<ContentNode> = Vec::new();
    let mut pending_text = String::new();

    for node in content {
        match node {
            ContentNode::Placeholder(_) => {
                if pending_text.is_empty() {
                    collapsed.push(node.clone());
                } else {
                    collapsed.push(ContentNode::text(pending_text.clone()));
                    pending_text.clear();
                }
            }
            ContentNode::Text(_) => pending_text.push_str(node.as_str()),
        }
    }
    if !pending_text.is_empty() {
        collapsed.push(ContentNode::text(pending_text));
    }

    collapsed
}

fn drop_empty(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut content = content.to_owned();
    content.retain(|node| match node {
        ContentNode::Placeholder(_) => true,
        ContentNode::Text(data) => !data.is_empty(),
    });
    content
}

fn trim_leading(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => ContentNode::text(data.trim_start_matches(trim)),
    }
}

fn trim_trailing(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => ContentNode::text(data.trim_end_matches(trim)),
    }
}

fn trim_both(node: &ContentNode, trim: &[char]) -> ContentNode {
    match node {
        ContentNode::Placeholder(_) => node.clone(),
        ContentNode::Text(data) => {
            let mut trimmed = data.trim_start_matches(trim);
            trimmed = trimmed.trim_end_matches(trim);
            ContentNode::text(trimmed)
        }
    }
}

pub fn trim_nodes(nodes: &mut Vec<ContentNode>, mode: TrimMode, trim: &[char]) -> Vec<ContentNode> {
    let mut nodes = nodes.to_owned();
    match mode {
        TrimMode::TrimAllLeading => nodes.iter().map(|node| trim_leading(node, trim)).collect(),
        TrimMode::TrimAllTrailing => nodes.iter().map(|node| trim_trailing(node, trim)).collect(),
        TrimMode::TrimAllBoth => nodes.iter().map(|node| trim_both(node, trim)).collect(),
        TrimMode::TrimOuterLeading => {
            match nodes.first() {
                Some(node) => nodes[0] = trim_leading(node, trim),
                _ => {}
            }
            nodes.to_vec()
        }
        TrimMode::TrimOuterTrailing => {
            match nodes.last() {
                Some(node) => {
                    nodes.to_owned().pop();
                    nodes.push(trim_trailing(node, trim))
                }
                _ => {}
            }
            nodes.to_vec()
        }
        TrimMode::TrimOuterBoth => {
            match nodes.first() {
                Some(node) => nodes[0] = trim_leading(node, trim),
                _ => {}
            }
            match nodes.last() {
                Some(node) => {
                    nodes.to_owned().pop();
                    nodes.push(trim_trailing(node, trim))
                }
                _ => {}
            }
            nodes.to_vec()
        }
    }
}

pub enum UnicodeNormalizationProfile {
    NFC,
    NFKC,
    NFD,
    NFKD,
    CJKCOMPATVARIANTS,
}

fn normalize_text(text: &str, profile: &UnicodeNormalizationProfile) -> String {
    match profile {
        UnicodeNormalizationProfile::NFC => text.nfc().collect(),
        UnicodeNormalizationProfile::NFKC => text.nfkc().collect(),
        UnicodeNormalizationProfile::NFD => text.nfd().collect(),
        UnicodeNormalizationProfile::NFKD => text.nfkd().collect(),
        UnicodeNormalizationProfile::CJKCOMPATVARIANTS => text.cjk_compat_variants().collect(),
    }
}

pub fn normalize_unicode(
    content: &[ContentNode],
    profile: &UnicodeNormalizationProfile,
) -> Vec<ContentNode> {
    content
        .iter()
        .map(|node| match node {
            ContentNode::Placeholder(_) => node.clone(),
            ContentNode::Text(data) => ContentNode::text(normalize_text(data, profile)),
        })
        .collect()
}

pub enum LanguageNormalizationProfile {
    LOWERCASE,
    UPPPERCASE,
}

pub fn normalize_language_tag(
    tag: &LanguageTag,
    profile: &LanguageNormalizationProfile,
) -> Result<LanguageTag, LanguageTagError> {
    match profile {
        LanguageNormalizationProfile::LOWERCASE => {
            LanguageTag::from_string(tag.as_str().to_ascii_lowercase())
        }
        LanguageNormalizationProfile::UPPPERCASE => {
            LanguageTag::from_string(tag.as_str().to_ascii_uppercase())
        }
    }
}
