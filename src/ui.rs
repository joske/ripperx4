use std::sync::Arc;
use std::sync::RwLock;
use std::thread;

use confy::ConfyError;
use gtk::Align;
use gtk::Frame;
use gtk::Separator;
use gtk::builders::BoxBuilder;
use gtk::builders::LabelBuilder;
use gtk::builders::TextBufferBuilder;
use gtk::builders::TextViewBuilder;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Box;
use gtk::Builder;
use gtk::Button;
use gtk::Dialog;
use gtk::DropDown;
use gtk::Orientation;
use gtk::Statusbar;
use gtk::TextView;
use gtk::prelude::*;

use discid::DiscId;

use crate::data::Config;
use crate::data::Data;
use crate::data::Encoder;
use crate::metadata::search_disc;
use crate::ripper::extract;

pub fn build_ui(app: &Application) {
    let data = Arc::new(RwLock::new(Data {
        ..Default::default()
    }));
    let ripping = Arc::new(RwLock::new(false));

    let builder = Builder::new();
    builder
        .add_from_resource("/ripperx4.ui")
        .expect("failed to load UI");
    
    let window: ApplicationWindow = builder.object("window").unwrap();
    window.set_application(Some(app));
    window.present();

    let exit_button: Button = builder.object("exit").unwrap();
    exit_button.connect_clicked(move |_| {
        window.close();
    });

    let title_text: TextView = builder.object("disc_title").unwrap();
    let artist_text: TextView = builder.object("disc_artist").unwrap();
    let year_text: TextView = builder.object("year").unwrap();
    let genre_text: TextView = builder.object("genre").unwrap();
    handle_disc(&title_text, data.clone(), &artist_text);

    let go_button: Button = builder.object("go_button").unwrap();
    let scroll: Box = builder.object("scroll").unwrap();
    let scan_button: Button = builder.object("scan_button").unwrap();
    handle_scan(
        scan_button,
        go_button.clone(),
        title_text,
        artist_text,
        year_text,
        genre_text,
        scroll,
        data.clone(),
    );

    let config_button: Button = builder.object("config_button").unwrap();
    handle_config(config_button);

    let stop_button: Button = builder.object("stop_button").unwrap();
    handle_stop(stop_button, ripping.clone());

    let status: Statusbar = builder.object("statusbar").unwrap();
    handle_go(ripping, go_button, status, data.clone());
}

fn handle_config(config_button: Button) {
    config_button.connect_clicked(move |_| {
        let cfg: Result<Config, ConfyError> = confy::load("ripperx4");
        let config = Arc::new(RwLock::new(cfg.unwrap()));
        let child = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .hexpand(true)
            .vexpand(true)
            .build();
        let frame = Frame::builder()
            .child(&child)
            .label("Configuration")
            .hexpand(true)
            .vexpand(true)
            .build();
        let path = TextView::builder()
            .visible(true)
            .hexpand(true)
            .build();
        path.buffer()
            .set_text(config.read().unwrap().encode_path.as_str());
        child.append(&path);
        let options = ["mp3", "ogg", "flac"];
        let combo = DropDown::from_strings(&options);
        let selected = match config.read().unwrap().encoder {
            Encoder::MP3 => 0,
            Encoder::OGG => 1,
            Encoder::FLAC => 2,
        };
        combo.set_selected(selected);
        child.append(&combo);
        let separator = Separator::builder()
            .vexpand(true)
            .build();
        child.append(&separator);
        let button_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .halign(Align::End)
            .build();
        let ok_button = Button::builder().label("Ok").build();
        button_box.append(&ok_button);
        let cancel_button = Button::builder().label("Cancel").build();
        button_box.append(&cancel_button);
        child.append(&button_box);

        let dialog = Dialog::builder()
            .title("Configuration")
            .modal(true)
            .child(&frame)
            .width_request(300)
            .build();
        let config_clone = config.clone();
        ok_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
                let buf = path.buffer();
                let new_path = path
                    .buffer()
                    .text(&buf.start_iter(), &buf.end_iter(), false);
                config_clone.write().unwrap().encode_path = new_path.to_string();
                let c = combo.selected();
                config_clone.write().unwrap().encoder = match c {
                    0 => Encoder::MP3,
                    1 => Encoder::OGG,
                    2 => Encoder::FLAC,
                    _ => panic!("invalid value"),
                };
                let c = config_clone.read().unwrap();
                confy::store("ripperx4", &*c).unwrap();
            dialog.close();
        }));
        cancel_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
            dialog.close();
        }));
        dialog.show();
    });
}

fn handle_disc(title_text: &TextView, data: Arc<RwLock<Data>>, artist_text: &TextView) {
    let title_buffer = title_text.buffer();
    let data_title = data.clone();
    title_buffer.connect_changed(glib::clone!(@weak title_buffer => move |_| {
        if data_title.write().unwrap().disc.is_some() {
            let new_title = title_buffer.text(&title_buffer.start_iter(), &title_buffer.end_iter(), false);
            data_title.write().unwrap().disc.as_mut().unwrap().title = new_title.to_string();
        }
    }));
    let artist_buffer = artist_text.buffer();
    let data_artist = data.clone();
    artist_buffer.connect_changed(glib::clone!(@weak artist_buffer => move |_| {
        if data_artist.write().unwrap().disc.is_some() {
            let new_artist = artist_buffer.text(&artist_buffer.start_iter(), &artist_buffer.end_iter(), false);
            data_artist.write().unwrap().disc.as_mut().unwrap().artist = new_artist.to_string();
        }
    }));
}

fn handle_stop(stop_button: Button, ripping: Arc<RwLock<bool>>) {
    stop_button.connect_clicked(move |_| {
        println!("stop");
        let mut ripping = ripping.write().unwrap();
        if *ripping {
            *ripping = false;
        }
    });
}

fn handle_scan(
    scan_button: Button,
    go_button: Button,
    title_text: TextView,
    artist_text: TextView,
    year_text: TextView,
    genre_text: TextView,
    scroll: Box,
    data: Arc<RwLock<Data>>,
) {
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

        println!("Scanned: {:?}", discid);
        println!("id={}", discid.id());
        println!("freedbid={}", discid.freedb_id());
        if let Ok(disc) = search_disc(&discid) {
            println!("disc:{}", disc.title);
            title_text.buffer().set_text(&disc.title.clone().as_str());
            artist_text.buffer().set_text(&disc.artist.clone().as_str());
            if (&disc).year.is_some() {
                year_text.buffer().set_text(&(&disc).year.unwrap().to_string());
            }
            if (&disc).genre.is_some() {
                genre_text.buffer().set_text(&disc.genre.clone().unwrap().clone().as_str());
            }
            data.write().unwrap().disc = Some(disc);
            // here we know how many tracks there are
            let tracks = discid.last_track_num() - discid.first_track_num() + 1;
            for i in 0..tracks {
                let hbox = BoxBuilder::new()
                    .orientation(Orientation::Horizontal)
                    .vexpand(false)
                    .hexpand(true)
                    .spacing(50)
                    .build();
                let label_text = format!("Track {}", i + 1);
                let label = LabelBuilder::new().label(&label_text).build();
                hbox.append(&label);

                let r = data.read().unwrap();
                let d = r.disc.as_ref().unwrap();
                let title = d.tracks[i as usize].title.as_str();
                let buffer = TextBufferBuilder::new().text(&title).build();
                let name = format!("{}", i);
                let tb = TextViewBuilder::new()
                    .name(&name)
                    .buffer(&buffer)
                    .hexpand(true)
                    .build();
                let data_changed = data.clone();
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
        go_button.set_sensitive(true);
    });
}

fn handle_go(
    ripping: Arc<RwLock<bool>>,
    go_button: Button,
    status: Statusbar,
    data: Arc<RwLock<Data>>,
) {
    let ripping_clone2 = ripping.clone();
    go_button.connect_clicked(glib::clone!(@weak status => move |_| {
        let mut ripping = ripping_clone2.write().unwrap();
        if !*ripping {
            *ripping = true;
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
    }));
}
