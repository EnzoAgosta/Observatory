//! Content-addressed identity for atoms.
//!
//! `AtomId = SHA-256(serialize(normalize(collapse(atom))))` (D4, D5, D18).
//! Nothing but normalized content and the canonical language tag is fed to the
//! hash: direction, domain, provenance and the like are observations, not
//! identity.
//!
//! This module currently implements **structural collapse** (Phase 1b.a) — the
//! fixed step that merges adjacent text runs and drops empty ones before
//! hashing. Content normalization (Phase 1d) and the serialization + SHA-256
//! (Phase 1b.b) follow.

use crate::ir::ContentNode;

/// Collapses a content sequence into its canonical structural form: adjacent
/// text runs are merged into one, and empty text runs are dropped (D17).
///
/// Placeholders are never merged or dropped — their count and position are
/// significant (D16). This is the structural step of computing an `AtomId`; it
/// deliberately does *not* normalize text *content* (Unicode form, whitespace,
/// case), which is the separate, configurable concern of [`crate::normalize`]
/// (Phase 1d).
///
/// Collapse is idempotent: `collapse(&collapse(x)) == collapse(x)`.
pub fn collapse(content: &[ContentNode]) -> Vec<ContentNode> {
    let mut collapsed: Vec<ContentNode> = Vec::new();
    let mut pending_text = String::new();

    for node in content {
        if node.is_placeholder() {
            flush_text(&mut pending_text, &mut collapsed);
            collapsed.push(node.clone());
        } else {
            pending_text.push_str(node.data());
        }
    }
    flush_text(&mut pending_text, &mut collapsed);

    collapsed
}

/// Pushes the accumulated text as a single node unless it is empty, and clears
/// the accumulator.
fn flush_text(pending: &mut String, out: &mut Vec<ContentNode>) {
    if !pending.is_empty() {
        out.push(ContentNode::text(std::mem::take(pending)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_adjacent_text_into_one_node() {
        let collapsed = collapse(&[ContentNode::text("Hello, "), ContentNode::text("world")]);
        assert_eq!(collapsed, [ContentNode::text("Hello, world")]);
    }

    #[test]
    fn drops_empty_text_runs() {
        let collapsed = collapse(&[
            ContentNode::text(""),
            ContentNode::text("x"),
            ContentNode::text(""),
        ]);
        assert_eq!(collapsed, [ContentNode::text("x")]);
    }

    #[test]
    fn placeholders_split_text_runs() {
        let collapsed = collapse(&[
            ContentNode::text("a"),
            ContentNode::placeholder("<x/>"),
            ContentNode::text("b"),
        ]);
        assert_eq!(
            collapsed,
            [
                ContentNode::text("a"),
                ContentNode::placeholder("<x/>"),
                ContentNode::text("b"),
            ]
        );
    }

    #[test]
    fn adjacent_placeholders_are_preserved() {
        let collapsed = collapse(&[
            ContentNode::placeholder("<x id=1/>"),
            ContentNode::placeholder("<x id=2/>"),
        ]);
        assert_eq!(collapsed.len(), 2);
        assert!(collapsed.iter().all(ContentNode::is_placeholder));
    }

    #[test]
    fn empty_placeholder_is_preserved() {
        let collapsed = collapse(&[ContentNode::placeholder("")]);
        assert_eq!(collapsed, [ContentNode::placeholder("")]);
    }

    #[test]
    fn distinct_chunkings_collapse_identically() {
        // The precursor to "same AtomId": different text chunkings reduce to the
        // same canonical structure (D17).
        let a = collapse(&[ContentNode::text("a"), ContentNode::text("b")]);
        let b = collapse(&[ContentNode::text("ab")]);
        assert_eq!(a, b);
    }

    #[test]
    fn collapse_is_idempotent() {
        let once = collapse(&[
            ContentNode::text("a"),
            ContentNode::text(""),
            ContentNode::text("b"),
            ContentNode::placeholder("<x/>"),
            ContentNode::placeholder(""),
            ContentNode::text("c"),
        ]);
        let twice = collapse(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn collapse_preserves_the_joined_text() {
        // Collapse must not change what the atom reconstructs to.
        let raw = [
            ContentNode::text("Click "),
            ContentNode::text(""),
            ContentNode::placeholder("<g id=1>"),
            ContentNode::text("here"),
            ContentNode::placeholder("</g>"),
        ];
        let joined: String = raw.iter().map(ContentNode::data).collect();
        let collapsed_joined: String = collapse(&raw).iter().map(ContentNode::data).collect();
        assert_eq!(joined, collapsed_joined);
    }
}
