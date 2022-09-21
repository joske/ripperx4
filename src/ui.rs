use std::sync::Arc;
use std::sync::RwLock;
use std::thread;

use confy::ConfyError;
use gtk::builders::BoxBuilder;
use gtk::builders::LabelBuilder;
use gtk::builders::TextBufferBuilder;
use gtk::builders::TextViewBuilder;
use gtk::prelude::*;
use gtk::Align;
use gtk::Application;
use gtk::ApplicationWindow;
use gtk::Box;
use gtk::Builder;
use gtk::Button;
use gtk::ButtonsType;
use gtk::Dialog;
use gtk::DropDown;
use gtk::Frame;
use gtk::MessageDialog;
use gtk::MessageType;
use gtk::Orientation;
use gtk::Separator;
use gtk::Statusbar;
use gtk::TextView;

use discid::DiscId;

use crate::data::Config;
use crate::data::Data;
use crate::data::Encoder;
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

    handle_disc(data.clone(), builder.clone());

    handle_scan(data.clone(), builder.clone());

    let config_button: Button = builder.object("config_button").unwrap();
    handle_config(config_button);

    let stop_button: Button = builder.object("stop_button").unwrap();
    stop_button.set_sensitive(false);
    handle_stop(ripping.clone(), builder.clone());

    handle_go(ripping, data, builder);
}

fn handle_config(config_button: Button) {
    config_button.connect_clicked(move |_| {
        let cfg: Result<Config, ConfyError> = confy::load("ripperx4", None);
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
        let path = TextView::builder().visible(true).hexpand(true).build();
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
        let separator = Separator::builder().vexpand(true).build();
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
        ok_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
                let buf = path.buffer();
                let new_path = path
                    .buffer()
                    .text(&buf.start_iter(), &buf.end_iter(), false);
                config.write().unwrap().encode_path = new_path.to_string();
                let c = combo.selected();
                config.write().unwrap().encoder = match c {
                    0 => Encoder::MP3,
                    1 => Encoder::OGG,
                    2 => Encoder::FLAC,
                    _ => panic!("invalid value"),
                };
                let c = config.read().unwrap();
                confy::store("ripperx4", None, &*c).unwrap();
            dialog.close();
        }));
        cancel_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
            dialog.close();
        }));
        dialog.show();
    });
}

fn handle_disc(data: Arc<RwLock<Data>>, builder: Builder) {
    let title_text: TextView = builder.object("disc_title").unwrap();
    let artist_text: TextView = builder.object("disc_artist").unwrap();
    let title_buffer = title_text.buffer();
    let data_title = data.clone();
    title_buffer.connect_changed(glib::clone!(@weak title_buffer => move |_| {
        if data_title.write().unwrap().disc.is_some() {
            let new_title = title_buffer.text(&title_buffer.start_iter(), &title_buffer.end_iter(), false);
            data_title.write().unwrap().disc.as_mut().unwrap().title = new_title.to_string();
        }
    }));
    let artist_buffer = artist_text.buffer();
    let data_artist = data;
    artist_buffer.connect_changed(glib::clone!(@weak artist_buffer => move |_| {
        if data_artist.write().unwrap().disc.is_some() {
            let new_artist = artist_buffer.text(&artist_buffer.start_iter(), &artist_buffer.end_iter(), false);
            data_artist.write().unwrap().disc.as_mut().unwrap().artist = new_artist.to_string();
        }
    }));
}

fn handle_stop(ripping: Arc<RwLock<bool>>, builder: Builder) {
    let stop_button: Button = builder.object("stop_button").unwrap();
    stop_button.connect_clicked(move |_| {
        println!("stop");
        let mut ripping = ripping.write().unwrap();
        if *ripping {
            *ripping = false;
            let stop_button: Button = builder.object("stop_button").unwrap();
            stop_button.set_sensitive(false);
            let go_button: Button = builder.object("go_button").unwrap();
            go_button.set_sensitive(true); //
            let scan_button: Button = builder.object("scan_button").unwrap();
            scan_button.set_sensitive(true);
        }
    });
}

fn handle_scan(data: Arc<RwLock<Data>>, builder: Builder) {
    let title_text: TextView = builder.object("disc_title").unwrap();
    let artist_text: TextView = builder.object("disc_artist").unwrap();
    let year_text: TextView = builder.object("year").unwrap();
    let genre_text: TextView = builder.object("genre").unwrap();
    let go_button: Button = builder.object("go_button").unwrap();
    let scroll: Box = builder.object("scroll").unwrap();
    let scan_button: Button = builder.object("scan_button").unwrap();
    scan_button.connect_clicked(move |_| {
        println!("Scan");
        let result = DiscId::read(Some(DiscId::default_device().as_str()));
        let discid = match result {
            Ok(d) => d,
            Err(_) => {
                // show_message("Disc not found!", MessageType::Error);
                // for testing on machine without CDROM drive: hardcode offsets of a dire straits disc
                let offsets = [
                    298948, 183, 26155, 44233, 64778, 80595, 117410, 144120, 159913, 178520,
                    204803, 258763, 277218,
                ];
                DiscId::put(1, &offsets).unwrap()
            }
        };

        println!("Scanned: {:?}", discid);
        println!("id={}", discid.id());
        if let Ok(disc) = crate::musicbrainz::lookup(&discid.id()) {
            println!("disc:{}", disc.title);
            title_text.buffer().set_text(disc.title.as_str());
            artist_text.buffer().set_text(disc.artist.as_str());
            if disc.year.is_some() {
                year_text
                    .buffer()
                    .set_text(&(disc.year.unwrap().to_string()));
            }
            if disc.genre.is_some() {
                genre_text
                    .buffer()
                    .set_text(disc.genre.clone().unwrap().as_str());
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
                let buffer = TextBufferBuilder::new().text(title).build();
                let name = format!("{}", i);
                let tb = TextViewBuilder::new()
                    .name(&name)
                    .buffer(&buffer)
                    .hexpand(true)
                    .build();
                let data_changed = data.clone();
                buffer.connect_changed(glib::clone!(@weak buffer => move |_| {
                    let mut r = data_changed.write().unwrap();
                    let d = &mut r.disc.as_mut().unwrap();
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
        } else {
            show_message("Disc not found!", MessageType::Error);
        }
        go_button.set_sensitive(true);
    });
}

fn show_message(message: &str, typ: MessageType) {
    let dialog = MessageDialog::builder()
        .title("Error")
        .modal(true)
        .buttons(ButtonsType::Ok)
        .message_type(typ)
        .text(message)
        .width_request(300)
        .build();
    dialog.connect_response(glib::clone!(@weak dialog => move |_, _| {
        dialog.close();
    }));
    dialog.show();
}

fn handle_go(ripping_arc: Arc<RwLock<bool>>, data: Arc<RwLock<Data>>, builder: Builder) {
    let go_button: Button = builder.object("go_button").unwrap();
    go_button.set_sensitive(false);
    let status: Statusbar = builder.object("statusbar").unwrap();
    let stop_button: Button = builder.object("stop_button").unwrap();
    go_button.connect_clicked(glib::clone!(@weak status => move |_| {
        let mut ripping = ripping_arc.write().unwrap();
        if !*ripping {
            stop_button.set_sensitive(true);
            let go_button: Button = builder.object("go_button").unwrap();
            go_button.set_sensitive(false);
            let scan_button: Button = builder.object("scan_button").unwrap();
            scan_button.set_sensitive(false);
            *ripping = true;
            let context_id = status.context_id("foo");
            let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
            let ripping_clone3 = ripping_arc.clone();
            thread::spawn(glib::clone!(@weak data => move || {
                let data_go = data;
                if let Some(disc) = &data_go.read().unwrap().disc {
                    extract(disc, &tx, ripping_clone3);
                    println!("done");
                    let _ = tx.send("done".to_owned());
                };
            }));
            let scan_button_clone = scan_button;
            let go_button_clone = go_button;
            let stop_button_clone = stop_button.clone();
            rx.attach(None, move |value| {
                let s = value;
                {
                    status.pop(context_id);
                    status.push(context_id, s.as_str());
                    if s == "done" {
                        scan_button_clone.set_sensitive(true);
                        go_button_clone.set_sensitive(true);
                        stop_button_clone.set_sensitive(false);
                        return glib::Continue(false);
                    }
                    glib::Continue(true)
                }
            });
        }
    }));
}
