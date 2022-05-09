use std::sync::Arc;
use std::sync::RwLock;
use std::thread;

use data::Data;
use discid::DiscId;
use gtk::Box;
use gtk::Orientation;
use gtk::TextView;
use gtk::builders::BoxBuilder;
use gtk::builders::LabelBuilder;
use gtk::builders::TextBufferBuilder;
use gtk::builders::TextViewBuilder;
use gtk::gio::resources_register_include;
use gtk::prelude::*;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Builder;
use gtk::Button;
use gtk::Statusbar;
use ripper::extract;

use crate::metadata::search_disc;

mod data;
mod metadata;
mod ripper;

pub fn main() {
    resources_register_include!("ripperx4.gresource")
        .expect("Failed to register resources.");

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
    let data = Arc::new(RwLock::new(Data {
        ..Default::default()
    }));
    let ripping = Arc::new(RwLock::new(false));

    let builder = Builder::new();
    builder.add_from_resource("/ripperx4.ui").expect("failed to load UI");
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
    let scroll: Box = builder.object("scroll").unwrap();
    let title_text : TextView = builder.object("disc_title").unwrap();
    let artist_text : TextView = builder.object("disc_artist").unwrap();

    let data_scan = data.clone();
    scan_button.connect_clicked(move |_| {
        println!("Scan");
        let result = DiscId::read(Some(DiscId::default_device().as_str()));
        let discid = match result {
            Ok(d) => d,
            Err(_) => {
                // for testing on machine without CDROM drive: hardcode offset of a dire straits disc
                let offsets = [
                    185700, 150, 18051, 42248, 57183, 75952, 89333, 114384, 142453, 163641,
                ];
                DiscId::put(1, &offsets).unwrap()
            }
        };
        // here we know how many tracks there are
        
        println!("Scanned: {:?}", discid);
        println!("id={}", discid.id());
        println!("freedbid={}", discid.freedb_id());
        if let Ok(disc) = search_disc(&discid) {
            println!("disc:{}", disc.title);
            title_text.buffer().set_text(&disc.title.clone().as_str());
            artist_text.buffer().set_text(&disc.artist.clone().as_str());
            data_scan.write().unwrap().disc = Some(disc);
            let tracks = discid.last_track_num() - discid.first_track_num() + 1;
            for i in 0..tracks {
                let hbox = BoxBuilder::new().orientation(Orientation::Horizontal).vexpand(false).hexpand(true).spacing(50).build();
                let label_text = format!("Track {}", i + 1);
                let label = LabelBuilder::new().label(&label_text).build();
                hbox.append(&label);

                let r = data_scan.read().unwrap();
                let d = r.disc.as_ref().unwrap();
                let title = d.tracks[i as usize].title.as_str();
                let buffer = TextBufferBuilder::new().text(&title).build();
                let name = format!("{}", i);
                let tb = TextViewBuilder::new().name(&name).buffer(&buffer).hexpand(true).build();
                let data_changed = data_scan.clone();
                buffer.connect_changed(glib::clone!(@weak buffer => move |_| {
                    let mut r = data_changed.write().unwrap();
                    let ref mut d = r.disc.as_mut().unwrap();
                    let tracks = &mut d.tracks;
                    let mut track = &mut tracks[i as usize];
                    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                    println!("{}", &text);
                    track.title = text.to_string();
                    println!("{}", &track.title);
                }));
                hbox.append(&tb);
                tb.show();
                scroll.append(&hbox);
                hbox.show();
            }
            scroll.show();
        }
        go_button_clone.set_sensitive(true);
    });

   let ripping_clone = ripping.clone();
   let stop_button: Button = builder.object("stop_button").unwrap();
   stop_button.connect_clicked(move |_| {
        println!("stop");
        let mut ripping = ripping_clone.write().unwrap();
        if *ripping {
            *ripping = false;
        }
    });

    let ripping_clone2 = ripping.clone();
    go_button.connect_clicked(move |_| {
        let mut ripping = ripping_clone2.write().unwrap();
        if !*ripping {
            *ripping = true;
            let status: Statusbar = builder.object("statusbar").unwrap();
            let context_id = status.context_id("foo");
            let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
            let ripping_clone3 = ripping_clone2.clone();
            thread::spawn(glib::clone!(@weak data => move || {
                let data_go = data.clone();
                if let Some(disc) = &data_go.read().unwrap().disc {
                    extract(&disc, &tx, ripping_clone3);
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
        }
    });
}
