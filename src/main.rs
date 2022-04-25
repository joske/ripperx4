use std::thread;

use glib::clone;
use gtk::Builder;
use gtk::ResponseType;
use gtk::Window;
use gtk::ffi::GtkBuilder;
use gtk::prelude::*;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Button;
use gtk::Dialog;
use ripper::extract;

mod ripper;

pub fn main() {
    // Create a new application
    let app = Application::builder()
        .application_id("be.sourcery.ripperx4")
        .build();

    // Connect to "activate" signal of `app`
    app.connect_activate(build_ui);

    // Run the application
    app.run();
}

fn build_ui(app: &Application) {
    let builder = Builder::from_resource("ripperx4.ui");
    // Create a button with label and margins
    let exitButton : Button = builder.object("exit").unwrap();
    let goButton : Button = builder.object("exit").unwrap();

    // let window: ApplicationWindow =ApplicationWindow::builder().application(app).build();
    goButton.connect_clicked(move |_| {
        thread::spawn(move || {
            extract();
        });
    });

    // Create a window
    let window : Window = builder.object("window").unwrap();

    // Present window
    window.present();
}
