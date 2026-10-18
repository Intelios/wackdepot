//! Keeping things on screen while they leave. A list (or a single overlay)
//! syncs its current items each render; removed items linger as `Exiting`
//! with their last data until their exit animation has run.

use std::hash::Hash;

/// Where an item is in its life.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// Seen for the first time this sync (or still within its entrance).
    Entering,
    Present,
    /// Gone from the source since time `since` (seconds, motion clock).
    Exiting {
        since: f64,
    },
}

#[derive(Debug, Clone)]
pub struct Item<K, T> {
    pub key: K,
    pub data: T,
    pub phase: Phase,
}

/// The items to draw: present ones in source order, exiting ones kept near
/// where they were.
#[derive(Debug, Clone)]
pub struct Presence<K, T> {
    items: Vec<Item<K, T>>,
    seeded: bool,
}

impl<K, T> Default for Presence<K, T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            seeded: false,
        }
    }
}

impl<K: Clone + Eq + Hash, T: Clone> Presence<K, T> {
    /// Reconcile with the current source. The very first sync enters
    /// nothing (everything is already there); later syncs mark new keys
    /// `Entering` and vanished ones `Exiting`.
    pub fn sync(&mut self, next: impl IntoIterator<Item = (K, T)>, now: f64, exit_secs: f64) {
        let next: Vec<(K, T)> = next.into_iter().collect();
        let mut out: Vec<Item<K, T>> = Vec::with_capacity(next.len());
        let mut old = std::mem::take(&mut self.items);
        // Drop exits that have finished.
        old.retain(|i| !matches!(i.phase, Phase::Exiting { since } if now - since >= exit_secs));

        let mut old_iter = old.into_iter().peekable();
        let mut pending: Vec<Item<K, T>> = Vec::new();
        for (key, data) in next {
            // Carry over any exiting/removed old items that sat before this key.
            while let Some(o) = old_iter.peek() {
                if o.key == key {
                    break;
                }
                let o = old_iter.next().expect("peeked");
                pending.push(o);
            }
            let prev = old_iter.next_if(|o| o.key == key);
            for mut p in pending.drain(..) {
                if !matches!(p.phase, Phase::Exiting { .. }) {
                    p.phase = Phase::Exiting { since: now };
                }
                out.push(p);
            }
            let phase = match prev {
                Some(p) if !matches!(p.phase, Phase::Exiting { .. }) => Phase::Present,
                _ if !self.seeded => Phase::Present,
                _ => Phase::Entering,
            };
            out.push(Item { key, data, phase });
        }
        for mut o in old_iter {
            if !matches!(o.phase, Phase::Exiting { .. }) {
                o.phase = Phase::Exiting { since: now };
            }
            out.push(o);
        }
        // An item can both re-enter and still be in `out` as an exit; keep
        // the live one.
        let live: Vec<K> = out
            .iter()
            .filter(|i| !matches!(i.phase, Phase::Exiting { .. }))
            .map(|i| i.key.clone())
            .collect();
        out.retain(|i| !matches!(i.phase, Phase::Exiting { .. }) || !live.contains(&i.key));
        self.items = out;
        self.seeded = true;
    }

    pub fn items(&self) -> &[Item<K, T>] {
        &self.items
    }

    /// Whether anything is still leaving.
    pub fn has_exits(&self) -> bool {
        self.items
            .iter()
            .any(|i| matches!(i.phase, Phase::Exiting { .. }))
    }
}
