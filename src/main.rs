use std::thread;

use glib::clone;
use gtk::ResponseType;
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
    // Create a button with label and margins
    let button = Button::builder()
        .label("Press me!")
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // let window: ApplicationWindow =ApplicationWindow::builder().application(app).build();
    button.connect_clicked(move |button| {
        // let dialog = Dialog::with_buttons(
        //     Some("Done!"),
        //     Some(&window),
        //     gtk::DialogFlags::MODAL,
        //     &[("Yes", ResponseType::Yes)],
        // );
    
        // dialog.connect_response(clone!(@weak window => move |dialog, response| {
        //     dialog.close();
        // }));
        thread::spawn(move || {
            extract();
            // dialog.present();
        });
    });

    // Create a window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("My GTK App")
        .child(&button)
        .build();

    // Present window
    window.present();
}
