//! Fuzzy matching of the "same song" across services.
//!
//! Used when exporting a mix to another platform and when resolving AI song
//! ideas to real tracks. ISRC matches win outright; otherwise we compare
//! normalized titles and artists and nudge the score by duration.

use crate::model::Track;

/// Minimum score for an automatic match.
pub const MATCH_THRESHOLD: f32 = 0.72;

/// Lowercase, drop "feat."/remaster noise, keep letters and digits only.
pub fn normalize_title(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut depth = 0usize;
    let mut bracket = String::new();
    for ch in lower.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                bracket.clear();
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if !is_noise(&bracket) {
                    out.push(' ');
                    out.push_str(&bracket);
                }
                bracket.clear();
            }
            _ if depth > 0 => bracket.push(ch),
            _ => out.push(ch),
        }
    }
    // " - 2011 Remaster", " - Radio Edit" style suffixes.
    let out = match out.split_once(" - ") {
        Some((head, tail)) if is_noise(tail) => head.to_owned(),
        _ => out,
    };
    squash(&out)
}

pub fn normalize_artist(s: &str) -> String {
    let lower = s.to_lowercase();
    let cut = ["feat.", "ft.", " featuring ", " with ", " x ", " & ", ","]
        .iter()
        .filter_map(|sep| lower.find(sep))
        .min()
        .unwrap_or(lower.len());
    squash(lower[..cut].trim_start_matches("the "))
}

fn is_noise(fragment: &str) -> bool {
    const NOISE: [&str; 12] = [
        "feat",
        "ft.",
        "with ",
        "remaster",
        "deluxe",
        "bonus",
        "explicit",
        "clean",
        "radio edit",
        "single version",
        "album version",
        "mono",
    ];
    NOISE.iter().any(|n| fragment.contains(n))
}

fn squash(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalized Levenshtein similarity in `[0, 1]`.
pub fn similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let dist = prev[b.len()] as f32;
    1.0 - dist / a.len().max(b.len()) as f32
}

/// How well `candidate` matches the wanted title/artist (0–1).
pub fn match_score(title: &str, artist: &str, duration_ms: Option<u64>, candidate: &Track) -> f32 {
    let want_title = normalize_title(title);
    let got_title = normalize_title(&candidate.title);
    let mut title_sim = similarity(&want_title, &got_title);
    // "Song" vs "Song (Live at …)" — containment is a decent signal.
    if title_sim < 0.9
        && !want_title.is_empty()
        && (got_title.starts_with(&want_title) || want_title.starts_with(&got_title))
    {
        title_sim = title_sim.max(0.85);
    }

    let want_artist = normalize_artist(artist);
    let artist_sim = if want_artist.is_empty() {
        0.5
    } else {
        candidate
            .artists
            .iter()
            .map(|a| {
                let got = normalize_artist(a);
                if got.contains(&want_artist) || (!got.is_empty() && want_artist.contains(&got)) {
                    1.0
                } else {
                    similarity(&want_artist, &got)
                }
            })
            .fold(0.0f32, f32::max)
    };

    let mut score = 0.6 * title_sim + 0.4 * artist_sim;
    if let (Some(want), true) = (duration_ms, candidate.duration_ms > 0) {
        let diff = want.abs_diff(candidate.duration_ms);
        if diff <= 3_000 {
            score += 0.08;
        } else if diff > 30_000 {
            score -= 0.15;
        }
    }
    score.clamp(0.0, 1.0)
}

/// Pick the best candidate above [`MATCH_THRESHOLD`].
pub fn best_match<'a>(
    title: &str,
    artist: &str,
    duration_ms: Option<u64>,
    candidates: &'a [Track],
) -> Option<(&'a Track, f32)> {
    candidates
        .iter()
        .map(|c| (c, match_score(title, artist, duration_ms, c)))
        .filter(|(_, s)| *s >= MATCH_THRESHOLD)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Playability, ProviderKind};

    fn track(title: &str, artist: &str, ms: u64) -> Track {
        Track {
            provider: ProviderKind::Spotify,
            id: "x".into(),
            title: title.into(),
            artists: vec![artist.into()],
            album: None,
            duration_ms: ms,
            artwork: None,
            artwork_large: None,
            isrc: None,
            permalink: None,
            preview_url: None,
            playability: Playability::Full,
        }
    }

    #[test]
    fn normalizes_noise() {
        assert_eq!(normalize_title("Here Comes the Sun - Remastered 2009"), "here comes the sun");
        assert_eq!(normalize_title("Stay (feat. Justin Bieber)"), "stay");
        assert_eq!(normalize_title("Midnight City [Live]"), "midnight city live");
        assert_eq!(normalize_artist("The Kid LAROI & Justin Bieber"), "kid laroi");
    }

    #[test]
    fn picks_the_right_version() {
        let candidates = vec![
            track("Holocene (Live)", "Bon Iver", 400_000),
            track("Holocene", "Bon Iver", 336_000),
            track("Holocene", "Some Cover Band", 336_000),
        ];
        let (best, score) = best_match("Holocene", "Bon Iver", Some(337_000), &candidates).unwrap();
        assert_eq!(best.artists[0], "Bon Iver");
        assert_eq!(best.duration_ms, 336_000);
        assert!(score > 0.95);
    }

    #[test]
    fn rejects_unrelated() {
        let candidates = vec![track("Bohemian Rhapsody", "Queen", 354_000)];
        assert!(best_match("Holocene", "Bon Iver", None, &candidates).is_none());
    }
}
