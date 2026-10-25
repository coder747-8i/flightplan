//! Undo / redo for the plan: the route (waypoints, fuel stops, altitude, departure, ground
//! time) and the fleet (aircraft edits and loading). Snapshots are taken by watching for
//! changes, so every way of editing is covered; a burst of changes (dragging a number, typing)
//! within a moment of each other is one step.

use crate::model::aircraft::Aircraft;
use crate::model::route::Route;
use std::time::{Duration, Instant};

const LIMIT: usize = 100;
/// changes closer together than this are one undo step
const BURST: Duration = Duration::from_millis(800);

#[derive(Clone)]
pub struct Snap {
    pub route: Route,
    pub fleet: Vec<Aircraft>,
    pub active: String,
}

fn key(s: &Snap) -> String {
    // the route's rev / editing flags aren't part of what the pilot sees
    format!("{}|{}|{}|{}|{:?}", serde_json::to_string(&s.route).unwrap_or_default(), s.route.cruise, s.route.departure, s.active, serde_json::to_string(&s.fleet).unwrap_or_default())
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    last: Option<(Snap, String)>,
    changed_at: Option<Instant>,
}

impl History {
    /// Call once a frame with the current state; records a step when it changed.
    pub fn observe(&mut self, now: Snap) {
        let k = key(&now);
        match &self.last {
            None => self.last = Some((now, k)),
            Some((_, lk)) if *lk == k => {}
            Some(_) => {
                let burst = self.changed_at.is_some_and(|t| t.elapsed() < BURST);
                if !burst {
                    if let Some((prev, _)) = self.last.take() {
                        self.undo.push(prev);
                        if self.undo.len() > LIMIT {
                            self.undo.remove(0);
                        }
                    }
                    self.redo.clear();
                }
                self.changed_at = Some(Instant::now());
                self.last = Some((now, k));
            }
        }
    }

    /// The state to go back to (the current one is kept for redo).
    pub fn undo(&mut self, current: Snap) -> Option<Snap> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        self.last = Some((prev.clone(), key(&prev)));
        self.changed_at = None;
        Some(prev)
    }

    pub fn redo(&mut self, current: Snap) -> Option<Snap> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.last = Some((next.clone(), key(&next)));
        self.changed_at = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::place::Place;

    fn snap(ids: &[&str], cruise: f64) -> Snap {
        let mut r = Route::default();
        r.replace(ids.iter().map(|id| Place::coord(33.0, -112.0)).enumerate().map(|(i, mut p)| {
            p.id = ids[i].to_string();
            p
        }).collect());
        r.cruise = cruise;
        Snap { route: r, fleet: vec![], active: String::new() }
    }

    #[test]
    fn undo_redo_steps() {
        let mut h = History::default();
        h.observe(snap(&["A"], 6500.0));
        h.observe(snap(&["A", "B"], 6500.0));
        h.changed_at = None; // as if time passed
        h.observe(snap(&["A", "B", "C"], 6500.0));
        let back = h.undo(snap(&["A", "B", "C"], 6500.0)).unwrap();
        assert_eq!(back.route.text(), "A B");
        let back = h.undo(back).unwrap();
        assert_eq!(back.route.text(), "A");
        assert!(h.undo(back.clone()).is_none());
        let fwd = h.redo(back).unwrap();
        assert_eq!(fwd.route.text(), "A B");
    }

    #[test]
    fn a_burst_is_one_step() {
        let mut h = History::default();
        h.observe(snap(&["A"], 6500.0));
        for c in [6600.0, 6700.0, 6800.0, 7500.0] {
            h.observe(snap(&["A"], c));
        }
        let back = h.undo(snap(&["A"], 7500.0)).unwrap();
        assert_eq!(back.route.cruise, 6500.0);
    }
}
