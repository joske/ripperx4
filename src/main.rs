use data::Config;
use gtk::{Application, gio::resources_register_include, prelude::*};
use log::warn;

use crate::util::write_config;

mod cdtext;
mod data;
mod gnudb;
mod musicbrainz;
#[cfg(target_os = "macos")]
mod paranoia;
mod ripper;
mod ui;
mod util;

/// Application entry point
/// Initializes logging, resources, `GStreamer`, configuration, and starts the GTK application
/// Returns `Ok(())` on success or an error boxed trait object on failure
/// # Errors
/// Returns an error if logging or resource registration fails
pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    resources_register_include!("ripperx4.gresource").expect("Failed to register resources.");

    // Initialize GStreamer once at startup
    if let Err(e) = gstreamer::init() {
        warn!("Failed to initialize GStreamer: {e}");
    }

    // Ensure config file exists
    if confy::load::<Config>("ripperx4", Some("ripperx4")).is_err() {
        let config = Config::default();
        write_config(&config);
    }

    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();
    app.connect_activate(ui::build);
    app.run();
    Ok(())
}
