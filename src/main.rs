use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;
use std::thread;

use data::Data;
use discid::DiscId;
use gtk::prelude::*;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Builder;
use gtk::Button;
use gtk::Statusbar;
use ripper::extract;

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
    let data = Arc::new(RwLock::new(Data {disc: None}));
    let builder = Builder::new();
    builder.add_from_file(Path::new("ripperx4.ui")).ok();
    let window: ApplicationWindow = builder.object("window").unwrap();
    window.set_application(Some(app));
    window.present();

    let exit_button: Button = builder.object("exit").unwrap();
    exit_button.connect_clicked(move |_| {
        window.close();
    });

    let go_button: Button = builder.object("go_button").unwrap();
    go_button.set_sensitive(false);
    let go_button_clone = go_button.clone();
    let scan_button: Button = builder.object("scan_button").unwrap();
    let data_scan = data.clone();
    scan_button.connect_clicked(move |_| {
        println!("Scan");
        let discid = DiscId::read(Some(DiscId::default_device().as_str())).unwrap();
        println!("Scanned: {:?}", discid);
        println!("id={}", discid.id());
        println!("freedbid={}", discid.freedb_id());
        if let Ok(disc) = search_disc(&discid) {
            println!("disc:{}", disc.title);
            data_scan.write().unwrap().disc = Some(disc);

        }
        go_button_clone.set_sensitive(true);
    });

    go_button.connect_clicked(move |_| {
        let status: Statusbar = builder.object("statusbar").unwrap();
        let context_id = status.context_id("foo");
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        thread::spawn(glib::clone!(@weak data => move || {
            let data_go = data.clone();
            if let Some(disc) = &data_go.read().unwrap().disc {
                extract(&disc, &tx);
                println!("done");
                let _ = tx.send("done".to_owned());
            };
        }));
        rx.attach(None, move |value| match value {
             s => {
                status.pop(context_id);
                status.push(context_id, s.as_str());
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
    });

}
