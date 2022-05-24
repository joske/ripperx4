use confy::ConfyError;
use data::Config;
use gtk::gio::resources_register_include;
use gtk::prelude::*;
use gtk::Application;

mod data;
mod ripper;
mod ui;

pub fn main() {
    resources_register_include!("ripperx4.gresource").expect("Failed to register resources.");

    let cfg: Result<Config, ConfyError> = confy::load("ripperx4");
    if cfg.is_err() {
        // make sure config exists
        let config = Config::default();
        confy::store("ripperx4", config).expect("failed to create config");
    }

    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();
    app.connect_activate(ui::build_ui);
    app.run();
}
