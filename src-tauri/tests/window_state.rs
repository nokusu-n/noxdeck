use noxdeck_lib::shell::window_state::{choose, read, Monitor, Placement, Rect};

fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn placement(normal: Rect) -> Placement {
    Placement {
        normal,
        maximized: false,
    }
}
fn monitors() -> Vec<Monitor> {
    vec![
        Monitor {
            work: rect(0, 40, 1920, 1040),
            scale: 1.0,
        },
        Monitor {
            work: rect(-1280, 0, 1280, 984),
            scale: 1.0,
        },
    ]
}
fn open(saved: Option<Placement>, monitors: &[Monitor]) -> Placement {
    choose(saved, monitors, (960.0, 720.0), (640.0, 480.0)).unwrap()
}

#[test]
fn first_start_uses_default_size_centered_on_primary_work_area() {
    assert_eq!(open(None, &monitors()), placement(rect(480, 200, 960, 720)));
}

#[test]
fn on_screen_rectangle_keeps_its_place_and_size_on_either_monitor() {
    for normal in [rect(130, 90, 800, 600), rect(-1100, 80, 900, 700)] {
        let saved = placement(normal);
        assert_eq!(open(Some(saved), &monitors()), saved);
    }
}

#[test]
fn disconnected_monitor_uses_remembered_size_centered_on_primary() {
    assert_eq!(
        open(Some(placement(rect(2300, 150, 800, 600))), &monitors()),
        placement(rect(560, 260, 800, 600))
    );
}

#[test]
fn large_off_screen_rectangle_is_cut_down_to_the_primary_work_area() {
    assert_eq!(
        open(
            Some(placement(rect(i32::MAX, i32::MIN, u32::MAX, u32::MAX))),
            &monitors()
        ),
        placement(rect(0, 40, 1920, 1040))
    );
}

#[test]
fn resolution_changed_or_a_clipped_title_bar_never_leaves_controls_out_of_reach() {
    assert_eq!(
        open(Some(placement(rect(130, 90, 2400, 1500))), &monitors()),
        placement(rect(0, 40, 1920, 1040))
    );
    assert_eq!(
        open(Some(placement(rect(130, -100, 800, 600))), &monitors()),
        placement(rect(560, 260, 800, 600))
    );
    assert_eq!(
        open(Some(placement(rect(-1800, 900, 800, 600))), &monitors()),
        placement(rect(-1280, 384, 800, 600))
    );
}

#[test]
fn maximized_state_keeps_the_normal_rectangle_for_restore() {
    let saved = Placement {
        normal: rect(130, 90, 800, 600),
        maximized: true,
    };
    assert_eq!(open(Some(saved), &monitors()), saved);
}

#[test]
fn logical_default_and_minimum_follow_monitor_scale() {
    let monitors = [Monitor {
        work: rect(0, 0, 2560, 1400),
        scale: 1.5,
    }];
    assert_eq!(open(None, &monitors), placement(rect(560, 160, 1440, 1080)));
    let result = open(Some(placement(rect(100, 100, 400, 300))), &monitors);
    assert_eq!(result.normal, rect(100, 100, 960, 720));
}

#[test]
fn empty_monitor_list_and_broken_or_missing_files_are_safe() {
    assert!(choose(None, &[], (960.0, 720.0), (640.0, 480.0)).is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window.json");
    assert_eq!(read(&path), None);
    std::fs::write(&path, "broken").unwrap();
    assert_eq!(read(&path), None);
    assert!(dir.path().join("window.json.broken").exists());
    let saved = Placement {
        normal: rect(-1100, 80, 900, 700),
        maximized: true,
    };
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(read(&path), Some(saved));
    std::fs::write(
        &path,
        serde_json::to_vec(&placement(rect(0, 0, 0, 0))).unwrap(),
    )
    .unwrap();
    assert_eq!(read(&path), None);
}
