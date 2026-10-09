#[test]
fn application_artwork_is_used_by_the_executable_and_bundle() {
    // Interior samples of the app artwork: dark background and light card.
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
        let card = sample(icon.width() / 3, icon.height() * 4 / 5);
        // The ICO's smallest palette image rounds the near-white card to white.
        assert!((240..=255).contains(&card[0]));
        assert_eq!(&card[1..], &[card[0], card[0], 255]);
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
    assert_ne!(
        include_bytes!("../icons/icon.ico").as_slice(),
        include_bytes!("../icons/noxdeck-tray.ico").as_slice()
    );
}
