use std::path::Path;
use std::thread;

use discid::DiscId;
use gtk::prelude::*;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Builder;
use gtk::Button;
use gtk::Statusbar;
use ripper::extract;

use crate::data::Disc;
use crate::data::Track;
use crate::metadata::search_disc;

mod ripper;
mod data;
mod metadata;

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
    let window: ApplicationWindow = builder.object("window").unwrap();
    window.set_application(Some(app));
    window.present();

    let exit_button: Button = builder.object("exit").unwrap();
    exit_button.connect_clicked(move |_| {
        window.close();
    });

    let scan_button: Button = builder.object("scan_button").unwrap();
    scan_button.connect_clicked(move |_| {
        let discid = DiscId::read(Some(DiscId::default_device().as_str())).unwrap();
        println!("Scanned: {:?}", discid);
        println!("id={}", discid.id());
        search_disc(discid.id().as_str());
        for t in discid.tracks() {
            println!("track: {:?}", t);
        }
    });

    let go_button: Button = builder.object("go_button").unwrap();
    go_button.connect_clicked(move |_| {
        let status: Statusbar = builder.object("statusbar").unwrap();
        let context_id = status.context_id("foo");
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        thread::spawn(move || {
            let _ = tx.send(Some(1));
            let disc = Disc {
                title: "Dire Straits".to_owned(),
                artist: "Dire Straits".to_owned(),
                ..Default::default()
            };
            let track = Track {
                number: 6,
                title: "Sultans Of Swing".to_owned(),
                artist: "Dire Straits".to_owned(),
                duration: 5*60 + 32,
                composer: None,             
            };
            extract(&disc, &track);
            println!("done");
            let _ = tx.send(None);
        });
        rx.attach(None, move |value| match value {
            Some(_) => {
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
