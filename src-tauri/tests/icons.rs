#[test]
fn application_artwork_is_used_by_the_executable_and_bundle() {
    // Interior samples of the app artwork: dark tile, a near-white upright card and the blue slanted card.
    // Kept away from edges so every generated size has the same samples.
    for icon in [
        tauri::include_image!("icons/32x32.png"),
        tauri::include_image!("icons/128x128.png"),
        tauri::include_image!("icons/icon.ico"),
    ] {
        let sample = |x: u32, y: u32| {
            let offset = ((y * icon.width() + x) * 4) as usize;
            &icon.rgba()[offset..offset + 4]
        };
        assert_eq!(
            sample(icon.width() / 2, icon.height() / 20),
            [34, 34, 34, 255]
        );
        let card = sample(icon.width() * 3 / 10, icon.height() / 2);
        // Small sizes blend the card's edge a little, and the ICO's palette image rounds it to white.
        assert!((236..=255).contains(&card[0]));
        assert_eq!(&card[1..], &[card[0], card[0], 255]);
        let slanted = sample(icon.width() / 2, icon.height() / 2);
        assert!(slanted[0] < 60 && slanted[1] > 120 && slanted[2] > 230 && slanted[3] == 255, "{slanted:?}");
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert!(config["bundle"]["icon"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "icons/icon.ico"));
    assert_eq!(
        config["bundle"]["windows"]["nsis"]["installerIcon"],
        "icons/icon.ico"
    );
    assert_eq!(
        config["bundle"]["windows"]["nsis"]["uninstallerIcon"],
        "icons/icon.ico"
    );
    assert!(include_str!("../src/shell.rs").contains("icons/noxdeck-tray.ico"));
    // The tray shows the executable's picture (the designer's decision); the two files stay apart
    // so the tray can diverge later.
    assert_eq!(
        include_bytes!("../icons/icon.ico").as_slice(),
        include_bytes!("../icons/noxdeck-tray.ico").as_slice()
    );
}
