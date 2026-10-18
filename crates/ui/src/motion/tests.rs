use super::easing::{EASE, EXIT, GLIDE, LINEAR, POP};
use super::presence::{Phase, Presence};
use super::spring::{CHIP_POP, DIGITS, FLY, GLIDE_PILL, LIST_ROW, Spring, TEXT_SWAP};
use super::store::Motion;

#[test]
fn bezier_endpoints_and_shape() {
    for curve in [EASE, POP, GLIDE, EXIT, LINEAR] {
        assert_eq!(curve.eval(0.0), 0.0);
        assert_eq!(curve.eval(1.0), 1.0);
    }
    assert!((LINEAR.eval(0.37) - 0.37).abs() < 1e-4);
    // Ease-out: well ahead of linear at the midpoint; ease-in: behind.
    assert!(EASE.eval(0.5) > 0.8);
    assert!(EXIT.eval(0.5) < 0.4);
    // POP overshoots before settling.
    let peak = (1..100)
        .map(|i| POP.eval(i as f32 / 100.0))
        .fold(0.0f32, f32::max);
    assert!(peak > 1.05, "POP peak {peak}");
}

#[test]
fn springs_settle_quickly() {
    for cfg in [GLIDE_PILL, LIST_ROW, CHIP_POP, DIGITS, TEXT_SWAP, FLY] {
        let s = Spring {
            cfg,
            x0: 100.0,
            v0: 0.0,
            t0: 0.0,
        };
        assert!(s.settled(1.2), "{cfg:?} still moving at 1.2s");
        assert!(!s.settled(0.02));
    }
}

#[test]
fn retarget_keeps_position_and_velocity() {
    let mut m = Motion::default();
    let k = ("x", 1);
    m.begin_at(0.0, false);
    assert_eq!(m.spring(k, 0.0, GLIDE_PILL), 0.0);
    m.end_frame();
    m.begin_at(0.0, false);
    m.spring(k, 100.0, GLIDE_PILL);
    m.end_frame();
    m.begin_at(0.05, false);
    let before = m.spring(k, 100.0, GLIDE_PILL);
    m.end_frame();
    // Retarget at the same instant: the value must not jump.
    m.begin_at(0.05, false);
    let after = m.spring(k, -50.0, GLIDE_PILL);
    assert!((before - after).abs() < 0.5, "jumped {before} -> {after}");
    assert!(m.end_frame(), "still moving, so a frame is requested");
}

#[test]
fn settled_store_requests_nothing_and_reduce_motion_snaps() {
    let mut m = Motion::default();
    let k = ("x", 2);
    m.begin_at(0.0, false);
    m.spring(k, 0.0, LIST_ROW);
    m.end_frame();
    m.begin_at(0.0, false);
    m.spring(k, 10.0, LIST_ROW);
    assert!(m.end_frame());
    m.begin_at(5.0, false);
    assert_eq!(m.spring(k, 10.0, LIST_ROW), 10.0);
    assert!(!m.end_frame(), "settled: no more frames");

    let mut r = Motion::default();
    r.begin_at(0.0, true);
    assert_eq!(r.spring_from(("y", 0), 0.0, 42.0, CHIP_POP), 42.0);
    assert_eq!(r.tween_from(("z", 0), 0.0, 1.0, 300.0, 0.0, EASE), 1.0);
    assert!(!r.end_frame());
}

#[test]
fn tweens_and_entrances() {
    let mut m = Motion::default();
    let k = ("fade", 0);
    m.begin_at(1.0, false);
    assert_eq!(m.tween_from(k, 0.0, 1.0, 200.0, 100.0, LINEAR), 0.0);
    m.end_frame();
    m.begin_at(1.2, false);
    let mid = m.tween_from(k, 0.0, 1.0, 200.0, 100.0, LINEAR);
    assert!((mid - 0.5).abs() < 0.01, "{mid}");
    m.end_frame();
    m.begin_at(2.0, false);
    assert_eq!(m.tween(k, 1.0, 200.0, LINEAR), 1.0);
}

#[test]
fn presence_keeps_exits_in_place() {
    let mut p: Presence<&str, u8> = Presence::default();
    p.sync([("a", 1), ("b", 2), ("c", 3)], 0.0, 0.2);
    assert!(p.items().iter().all(|i| i.phase == Phase::Present));
    p.sync([("a", 1), ("c", 3), ("d", 4)], 1.0, 0.2);
    let keys: Vec<_> = p.items().iter().map(|i| (i.key, i.phase)).collect();
    assert_eq!(keys[0], ("a", Phase::Present));
    assert_eq!(keys[1], ("b", Phase::Exiting { since: 1.0 }));
    assert_eq!(keys[2], ("c", Phase::Present));
    assert_eq!(keys[3], ("d", Phase::Entering));
    // After the exit duration the ghost is gone.
    p.sync([("a", 1), ("c", 3), ("d", 4)], 1.5, 0.2);
    assert_eq!(p.items().len(), 3);
    assert!(!p.has_exits());
}
