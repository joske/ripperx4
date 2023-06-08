use confy::ConfyError;
use data::Config;
use gtk::{gio::resources_register_include, prelude::*, Application};

mod data;
mod musicbrainz;
mod ripper;
mod ui;

pub fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("Failed to initialize logger.");
    resources_register_include!("ripperx4.gresource").expect("Failed to register resources.");

    let cfg: Result<Config, ConfyError> = confy::load("ripperx4", None);
    if cfg.is_err() {
        // make sure config exists
        let config = Config::default();
        confy::store("ripperx4", None, config).expect("failed to create config");
    }

    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();
    app.connect_activate(ui::build_ui);
    app.run();
}
