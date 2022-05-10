use gtk::gio::resources_register_include;
use gtk::prelude::*;
use gtk::Application;

mod data;
mod metadata;
mod ripper;
mod ui;

pub fn main() {
    resources_register_include!("ripperx4.gresource").expect("Failed to register resources.");

    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();
    app.connect_activate(ui::build_ui);
    app.run();
}

