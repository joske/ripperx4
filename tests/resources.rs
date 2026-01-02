use gtk::gio::{self, ResourceLookupFlags, resources_register_include};

#[test]
fn load_image_resources() {
    resources_register_include!("ripperx4.gresource").expect("register resources");
    for path in [
        "/images/go.png",
        "/images/ripperX.png",
        "/images/stop.png",
        "/images/exit.png",
        "/images/cddb.png",
        "/images/scan.png",
        "/images/config.png",
    ] {
        let bytes = gio::resources_lookup_data(path, ResourceLookupFlags::NONE)
            .unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
        assert!(
            !bytes.is_empty(),
            "resource {path} is empty even though it exists"
        );
    }
}
