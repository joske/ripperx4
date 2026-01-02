use data::Config;
use gtk::{Application, gio::resources_register_include, prelude::*};
use log::warn;

mod data;
mod musicbrainz;
mod ripper;
mod ui;
mod util;

pub fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("Failed to initialize logger.");

    resources_register_include!("ripperx4.gresource").expect("Failed to register resources.");

    // Initialize GStreamer once at startup
    if let Err(e) = gstreamer::init() {
        warn!("Failed to initialize GStreamer: {e}");
    }

    // Ensure config file exists
    if confy::load::<Config>("ripperx4", None).is_err() {
        let config = Config::default();
        if let Err(e) = confy::store("ripperx4", None, config) {
            warn!("Failed to create config: {e}");
        }
    }

    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();
    app.connect_activate(ui::build);
    app.run();
}
