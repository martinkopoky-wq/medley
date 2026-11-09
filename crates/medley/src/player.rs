//! Queue and transport state. Pure logic; the app wires it to the audio engine.

use medley_core::Track;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Loading,
    Playing,
    Paused,
}

/// The play order. `queue[index]` is the current track; everything after it
/// is "Up Next". Shuffle reorders only the upcoming part and remembers the
/// original order so turning it off restores it.
#[derive(Debug, Default)]
pub struct Player {
    pub queue: Vec<Track>,
    pub index: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// Where the current queue came from ("Liked Songs on Spotify", a mix name…).
    pub context: String,
    unshuffled: Option<Vec<Track>>,
}

impl Player {
    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.queue.get(i))
    }

    pub fn upcoming(&self) -> &[Track] {
        match self.index {
            Some(i) if i < self.queue.len() => &self.queue[i + 1..],
            Some(_) => &[],
            None => &self.queue,
        }
    }

    pub fn history(&self) -> &[Track] {
        match self.index {
            Some(i) => &self.queue[..i.min(self.queue.len())],
            None => &[],
        }
    }

    /// Replace the queue with `tracks` and start at `start`.
    pub fn load(&mut self, tracks: Vec<Track>, start: usize, context: impl Into<String>) -> Option<&Track> {
        self.context = context.into();
        self.unshuffled = None;
        if tracks.is_empty() {
            self.queue.clear();
            self.index = None;
            return None;
        }
        let start = start.min(tracks.len() - 1);
        self.queue = tracks;
        self.index = Some(start);
        if self.shuffle {
            self.shuffle_upcoming();
        }
        self.current()
    }

    /// Advance after a track ended (`user` = pressed Next, which skips repeat-one).
    pub fn advance(&mut self, user: bool) -> Option<&Track> {
        let i = self.index?;
        if self.repeat == Repeat::One && !user {
            return self.current();
        }
        if i + 1 < self.queue.len() {
            self.index = Some(i + 1);
        } else if self.repeat != Repeat::Off && !self.queue.is_empty() {
            self.index = Some(0);
            if self.shuffle {
                self.unshuffled = None;
                self.shuffle_all_but_current();
            }
        } else {
            return None;
        }
        self.current()
    }

    /// Step back (callers restart the current track instead when it's >3 s in).
    pub fn back(&mut self) -> Option<&Track> {
        let i = self.index?;
        if i > 0 {
            self.index = Some(i - 1);
        } else if self.repeat == Repeat::All && !self.queue.is_empty() {
            self.index = Some(self.queue.len() - 1);
        }
        self.current()
    }

    /// Jump to the n-th upcoming track.
    pub fn jump_upcoming(&mut self, n: usize) -> Option<&Track> {
        let target = self.index.map_or(n, |i| i + 1 + n);
        if target < self.queue.len() {
            self.index = Some(target);
        }
        self.current()
    }

    pub fn play_next(&mut self, track: Track) {
        let at = self.index.map_or(0, |i| i + 1).min(self.queue.len());
        self.queue.insert(at, track.clone());
        if let Some(orig) = &mut self.unshuffled {
            orig.push(track);
        }
    }

    pub fn enqueue(&mut self, tracks: impl IntoIterator<Item = Track>) {
        for t in tracks {
            if let Some(orig) = &mut self.unshuffled {
                orig.push(t.clone());
            }
            self.queue.push(t);
        }
    }

    pub fn remove_upcoming(&mut self, n: usize) -> Option<Track> {
        let at = self.index.map_or(n, |i| i + 1 + n);
        (at < self.queue.len()).then(|| {
            let t = self.queue.remove(at);
            if let Some(orig) = &mut self.unshuffled
                && let Some(pos) = orig.iter().position(|o| o.key() == t.key())
            {
                orig.remove(pos);
            }
            t
        })
    }

    /// Reorder within Up Next (indices relative to `upcoming()`).
    pub fn move_upcoming(&mut self, from: usize, to: usize) {
        let base = self.index.map_or(0, |i| i + 1);
        let (from, to) = (base + from, base + to);
        if from < self.queue.len() && to < self.queue.len() && from != to {
            let t = self.queue.remove(from);
            self.queue.insert(to, t);
        }
    }

    pub fn clear_upcoming(&mut self) {
        let keep = self.index.map_or(0, |i| i + 1);
        self.queue.truncate(keep);
        self.unshuffled = None;
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            self.shuffle_upcoming();
        } else if let Some(orig) = self.unshuffled.take() {
            // Restore the original order, continuing after the current track.
            let current_key = self.current().map(Track::key);
            let pos = current_key.and_then(|k| orig.iter().position(|t| t.key() == k));
            self.queue = orig;
            self.index = pos.or(self.index.map(|i| i.min(self.queue.len().saturating_sub(1))));
        }
    }

    fn shuffle_upcoming(&mut self) {
        use rand::seq::SliceRandom;
        if self.unshuffled.is_none() {
            self.unshuffled = Some(self.queue.clone());
        }
        let start = self.index.map_or(0, |i| i + 1);
        if start < self.queue.len() {
            self.queue[start..].shuffle(&mut rand::rng());
        }
    }

    fn shuffle_all_but_current(&mut self) {
        use rand::seq::SliceRandom;
        if self.queue.len() > 1 {
            self.queue[1..].shuffle(&mut rand::rng());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medley_core::{Playability, ProviderKind};

    fn t(id: &str) -> Track {
        Track {
            provider: ProviderKind::Spotify,
            id: id.into(),
            title: id.into(),
            artists: vec!["A".into()],
            album: None,
            duration_ms: 1000,
            artwork: None,
            artwork_large: None,
            isrc: None,
            permalink: None,
            preview_url: None,
            playability: Playability::Full,
        }
    }

    fn ids(p: &Player) -> Vec<String> {
        p.queue.iter().map(|t| t.id.clone()).collect()
    }

    #[test]
    fn advance_and_repeat() {
        let mut p = Player::default();
        p.load(vec![t("a"), t("b")], 0, "x");
        assert_eq!(p.advance(false).unwrap().id, "b");
        assert!(p.advance(false).is_none());
        p.repeat = Repeat::All;
        assert_eq!(p.advance(false).unwrap().id, "a");
        p.repeat = Repeat::One;
        assert_eq!(p.advance(false).unwrap().id, "a");
        assert_eq!(p.advance(true).unwrap().id, "b");
    }

    #[test]
    fn play_next_inserts_after_current() {
        let mut p = Player::default();
        p.load(vec![t("a"), t("b")], 0, "x");
        p.play_next(t("n"));
        assert_eq!(ids(&p), ["a", "n", "b"]);
        assert_eq!(p.upcoming().len(), 2);
    }

    #[test]
    fn shuffle_keeps_current_and_restores() {
        let mut p = Player::default();
        let tracks: Vec<_> = (0..30).map(|i| t(&i.to_string())).collect();
        p.load(tracks, 5, "x");
        p.set_shuffle(true);
        assert_eq!(p.current().unwrap().id, "5");
        assert_eq!(p.queue.len(), 30);
        p.advance(true);
        let now = p.current().unwrap().id.clone();
        p.set_shuffle(false);
        assert_eq!(p.current().unwrap().id, now);
        assert_eq!(ids(&p), (0..30).map(|i| i.to_string()).collect::<Vec<_>>());
    }

    #[test]
    fn upcoming_edits() {
        let mut p = Player::default();
        p.load(vec![t("a"), t("b"), t("c"), t("d")], 0, "x");
        p.move_upcoming(2, 0);
        assert_eq!(ids(&p), ["a", "d", "b", "c"]);
        assert_eq!(p.remove_upcoming(1).unwrap().id, "b");
        assert_eq!(p.jump_upcoming(1).unwrap().id, "c");
        p.clear_upcoming();
        assert!(p.upcoming().is_empty());
    }

    #[test]
    fn back_wraps_only_with_repeat_all() {
        let mut p = Player::default();
        p.load(vec![t("a"), t("b")], 0, "x");
        assert_eq!(p.back().unwrap().id, "a");
        p.repeat = Repeat::All;
        assert_eq!(p.back().unwrap().id, "b");
    }
}
