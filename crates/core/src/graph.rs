//! History graph lanes: where each commit's dot sits and which lines run
//! through its row. Pure — computed from [`LogEntry::parents`] over a log
//! in `--date-order` (children before parents).
//!
//! A row is drawn in two halves around its node: edges arrive from the top
//! edge of the row into the node, leave the node towards the bottom edge, or
//! pass straight through for lanes this commit doesn't touch.

use crate::commit::LogEntry;

/// One line segment through a row. Lane indices are columns, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// A lane passing through, top to bottom in the same column.
    Pass { lane: usize, color: usize },
    /// From the top of `lane` into this row's node.
    In { lane: usize, color: usize },
    /// From this row's node down to the bottom of `lane`.
    Out { lane: usize, color: usize },
    /// A lane bending from column `from` at the top to `to` at the bottom —
    /// a side branch folding back into the line it forked from.
    Shift {
        from: usize,
        to: usize,
        color: usize,
    },
}

/// The graph for one history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// Column of the commit's dot.
    pub node: usize,
    /// Colour index of the node's lane (an index into a fixed palette).
    pub color: usize,
    pub edges: Vec<Edge>,
    /// Columns this row spans (for sizing the gutter).
    pub width: usize,
}

/// Lay out lanes for `log` (newest first, parents after children).
pub fn layout(log: &[LogEntry]) -> Vec<GraphRow> {
    // Each lane remembers which commit it's waiting for and its colour.
    let mut lanes: Vec<Option<(String, usize)>> = Vec::new();
    let mut next_color = 0usize;
    let mut rows = Vec::with_capacity(log.len());

    for entry in log {
        let mut edges = Vec::new();
        let width_before = lanes.len();

        // Lanes arriving at this commit (its children's parent pointers).
        let incoming: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_ref().is_some_and(|(sha, _)| *sha == entry.sha))
            .map(|(i, _)| i)
            .collect();

        let (node, color) = match incoming.first() {
            Some(&i) => (i, lanes[i].as_ref().map(|(_, c)| *c).unwrap_or(0)),
            None => {
                // A branch tip nothing above points at: take a free column.
                let slot = free_slot(&mut lanes);
                let c = next_color;
                next_color += 1;
                (slot, c)
            }
        };
        for &i in &incoming {
            let c = lanes[i].as_ref().map(|(_, c)| *c).unwrap_or(color);
            edges.push(Edge::In { lane: i, color: c });
            lanes[i] = None;
        }

        // Lanes this commit doesn't touch pass straight through.
        for (i, lane) in lanes.iter().enumerate() {
            if let Some((_, c)) = lane
                && i != node
            {
                edges.push(Edge::Pass { lane: i, color: *c });
            }
        }

        // Parents: the first continues the node's own lane and colour; any
        // others join a lane already waiting for them or open a new one.
        for (pi, parent) in entry.parents.iter().enumerate() {
            if pi == 0 {
                match waiting_for(&lanes, parent) {
                    // Already expected further left — join that lane.
                    Some(existing) if existing < node => {
                        let c = lanes[existing].as_ref().map(|(_, c)| *c).unwrap_or(color);
                        edges.push(Edge::Out {
                            lane: existing,
                            color: c,
                        });
                    }
                    // Expected further right (a side branch forked here):
                    // keep this line in its column and fold the side lane in.
                    Some(existing) => {
                        let side = lanes[existing].take().map(|(_, c)| c).unwrap_or(color);
                        if let Some(e) = edges
                            .iter_mut()
                            .find(|e| matches!(e, Edge::Pass { lane, .. } if *lane == existing))
                        {
                            *e = Edge::Shift {
                                from: existing,
                                to: node,
                                color: side,
                            };
                        }
                        lanes[node] = Some((parent.clone(), color));
                        edges.push(Edge::Out { lane: node, color });
                    }
                    None => {
                        lanes[node] = Some((parent.clone(), color));
                        edges.push(Edge::Out { lane: node, color });
                    }
                }
                continue;
            }
            if let Some(existing) = waiting_for(&lanes, parent) {
                let c = lanes[existing].as_ref().map(|(_, c)| *c).unwrap_or(color);
                edges.push(Edge::Out {
                    lane: existing,
                    color: c,
                });
            } else {
                let slot = free_slot(&mut lanes);
                let c = next_color;
                next_color += 1;
                lanes[slot] = Some((parent.clone(), c));
                edges.push(Edge::Out {
                    lane: slot,
                    color: c,
                });
            }
        }

        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        rows.push(GraphRow {
            node,
            color,
            width: width_before.max(lanes.len()).max(node + 1),
            edges,
        });
    }
    rows
}

fn waiting_for(lanes: &[Option<(String, usize)>], sha: &str) -> Option<usize> {
    lanes
        .iter()
        .position(|l| l.as_ref().is_some_and(|(s, _)| s == sha))
}

/// The leftmost empty column, growing the lane list when all are taken.
fn free_slot(lanes: &mut Vec<Option<(String, usize)>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(sha: &str, parents: &[&str]) -> LogEntry {
        LogEntry {
            sha: sha.into(),
            short: sha.into(),
            author_name: String::new(),
            author_email: String::new(),
            authored_at: String::new(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            subject: String::new(),
            body: String::new(),
        }
    }

    #[test]
    fn linear_history_is_one_lane() {
        let rows = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert!(rows.iter().all(|r| r.node == 0 && r.width == 1));
        assert_eq!(rows[0].edges, vec![Edge::Out { lane: 0, color: 0 }]);
        assert_eq!(
            rows[1].edges,
            vec![
                Edge::In { lane: 0, color: 0 },
                Edge::Out { lane: 0, color: 0 }
            ]
        );
        assert_eq!(rows[2].edges, vec![Edge::In { lane: 0, color: 0 }]);
    }

    #[test]
    fn merge_opens_and_closes_a_lane() {
        // m merges f into b; f and b both come from a.
        let rows = layout(&[
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ]);
        assert_eq!(rows[0].node, 0);
        assert!(rows[0].edges.contains(&Edge::Out { lane: 1, color: 1 }));
        assert_eq!(rows[1].node, 1);
        assert!(rows[1].edges.contains(&Edge::Pass { lane: 0, color: 0 }));
        // f's parent a is awaited by lane 1; b also wants a, so the side lane
        // folds back into the main column and the main line stays put.
        assert_eq!(rows[2].node, 0);
        assert!(rows[2].edges.contains(&Edge::Shift {
            from: 1,
            to: 0,
            color: 1
        }));
        assert!(rows[2].edges.contains(&Edge::Out { lane: 0, color: 0 }));
        assert_eq!(rows[3].node, 0);
        assert_eq!(rows[3].width, 1);
    }

    #[test]
    fn octopus_and_branch_tips() {
        let rows = layout(&[
            c("tip", &["x"]),
            c("o", &["p1", "p2", "p3"]),
            c("p1", &["r"]),
            c("p2", &["r"]),
            c("p3", &["r"]),
            c("x", &["r"]),
            c("r", &[]),
        ]);
        // The unrelated tip and the octopus sit side by side.
        assert_eq!(rows[0].node, 0);
        assert_eq!(rows[1].node, 1);
        let outs = rows[1]
            .edges
            .iter()
            .filter(|e| matches!(e, Edge::Out { .. }))
            .count();
        assert_eq!(outs, 3);
        assert!(rows.iter().map(|r| r.width).max().unwrap() >= 4);
        // r collects every lane.
        let ins = rows[6]
            .edges
            .iter()
            .filter(|e| matches!(e, Edge::In { .. }))
            .count();
        assert!(ins >= 1);
    }

    #[test]
    fn truncated_log_leaves_lanes_open() {
        let rows = layout(&[c("b", &["a"]), c("x", &["missing"])]);
        assert_eq!(rows[1].node, 1);
        assert!(rows[1].edges.contains(&Edge::Pass { lane: 0, color: 0 }));
    }
}
