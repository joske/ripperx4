use std::path::Path;
use std::thread;

use glib::clone;
use gtk::ffi::GtkBuilder;
use gtk::prelude::*;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Builder;
use gtk::Button;
use gtk::Dialog;
use gtk::Image;
use gtk::Statusbar;
use gtk::Window;
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
    let builder = Builder::new();
    builder.add_from_file(Path::new("ripperx4.ui")).ok();
    let window: Window = builder.object("window").unwrap();
    window.set_application(Some(app));
    window.present();

    let exit_button: Button = builder.object("exit").unwrap();
    let image = Image::from_file("src/xpms/exit.xpm");
    exit_button.set_child(Some(&image));
    exit_button.connect_clicked(move |_| {
        window.close();
    });

    let go_button: Button = builder.object("go_button").unwrap();
    let image = Image::from_file("src/xpms/go.xpm");
    go_button.set_child(Some(&image));
    go_button.connect_clicked(move |_| {
        let status: Statusbar = builder.object("statusbar").unwrap();
        let context_id = status.context_id("foo");
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        thread::spawn(move || {
            let _ = tx.send(Some(1));
            extract();
            println!("done");
            let _ = tx.send(None);
        });
        rx.attach(None, move |value| match value {
            Some(value) => {
                status.push(context_id, "starting");
                glib::Continue(true)
            }
            None => {
                println!("received done");
                status.pop(context_id);
                status.push(context_id, "done");
                glib::Continue(false)
            }
        });
    });

}
