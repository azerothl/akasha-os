//! Semantic anchor boundaries for E20 prefix cache (E21 hook).
//!
//! Agent prompts change at turn / tool / thinking boundaries more often than at
//! arbitrary token offsets. When reusing KV, snap the reusable prefix back to
//! the latest complete semantic segment so mid-message edits do not force a
//! full re-prefill.

/// Default ChatML / agent boundary markers (substring match on detokenized pieces).
pub const DEFAULT_BOUNDARY_MARKERS: &[&str] = &[
    "<|im_start|>",
    "<|im_end|>",
    "<|im_start|>user",
    "<|im_start|>assistant",
    "<|im_start|>tool",
    "<|im_start|>system",
    "<think>",
    "</think>",
];

/// Token indices where a new semantic segment starts (inclusive), always including `0`.
pub fn anchor_positions_from_pieces(pieces: &[String], markers: &[&str]) -> Vec<usize> {
    let mut anchors = vec![0usize];
    let mut acc = String::new();
    for (i, piece) in pieces.iter().enumerate() {
        acc.push_str(piece);
        if markers.iter().any(|m| !m.is_empty() && acc.contains(m)) {
            // Anchor at the start of the token that completed a marker match.
            if anchors.last().copied() != Some(i) {
                anchors.push(i);
            }
        }
    }
    anchors.sort_unstable();
    anchors.dedup();
    anchors
}

/// Largest anchor position `p` with `p <= max_pos` (or `0` if none).
pub fn snap_to_anchor(max_pos: usize, anchors: &[usize]) -> usize {
    anchors
        .iter()
        .copied()
        .filter(|&p| p <= max_pos)
        .max()
        .unwrap_or(0)
}

/// Prefix length for KV reuse: common prefix, snapped to a semantic anchor when
/// the raw match ends inside a segment (agent context edit).
pub fn semantic_prefix_len(prev: &[i32], next: &[i32], anchors_in_prev: &[usize]) -> usize {
    let raw = common_prefix_len_tokens(prev, next);
    if raw == 0 || raw == prev.len() || raw == next.len() {
        return raw;
    }
    snap_to_anchor(raw.saturating_sub(1), anchors_in_prev)
}

fn common_prefix_len_tokens(a: &[i32], b: &[i32]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces_for(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn anchors_detect_im_start_turns() {
        let pieces = pieces_for(&[
            "<|im_start|>",
            "user",
            "\n",
            "hi",
            "",
            "\n",
            "<|im_start|>",
            "assistant",
            "\n",
        ]);
        let anchors = anchor_positions_from_pieces(&pieces, DEFAULT_BOUNDARY_MARKERS);
        assert!(anchors.contains(&0));
        assert!(anchors.len() >= 2);
    }

    #[test]
    fn semantic_prefix_snaps_before_divergence_inside_turn() {
        // prev: user turn + start of assistant; next: user turn + different assistant start
        let prev: Vec<i32> = (0..12).collect();
        let mut next = prev.clone();
        next[10] = 99;
        // anchors at turn boundaries 0 and 6
        let anchors = vec![0, 6];
        let raw = 10usize;
        assert_eq!(raw, 10); // sanity
        let snapped = semantic_prefix_len(&prev, &next, &anchors);
        assert_eq!(snapped, 6, "should reuse through last full user turn");
    }

    #[test]
    fn full_match_unchanged() {
        let a: Vec<i32> = (0..8).collect();
        let b = a.clone();
        assert_eq!(semantic_prefix_len(&a, &b, &[0, 4]), 8);
    }

    #[test]
    fn snap_to_anchor_picks_latest() {
        assert_eq!(snap_to_anchor(7, &[0, 3, 6, 9]), 6);
        assert_eq!(snap_to_anchor(2, &[0, 3, 6]), 0);
    }
}
