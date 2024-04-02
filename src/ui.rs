use crate::{
    data::{Config, Data, Encoder, Quality},
    ripper::extract,
};
use discid::DiscId;
use gtk::{
    prelude::*, Align, Application, ApplicationWindow, Box, Builder, Button, ButtonsType, Dialog,
    DropDown, Frame, Label, MessageDialog, MessageType, Orientation, Separator, Statusbar,
    TextBuffer, TextView,
};
use log::debug;
use std::{
    sync::{Arc, RwLock},
    thread,
};

pub fn build_ui(app: &Application) {
    let data = Arc::new(RwLock::new(Data {
        ..Default::default()
    }));
    let ripping = Arc::new(RwLock::new(false));

    let builder = Builder::new();
    builder
        .add_from_resource("/ripperx4.ui")
        .expect("failed to load UI");

    let window: ApplicationWindow = builder.object("window").expect("Failed to get widget");
    window.set_application(Some(app));
    window.present();

    let window_clone = window.clone();
    let exit_button: Button = builder.object("exit").expect("Failed to get widget");
    exit_button.connect_clicked(move |_| {
        window.close();
    });

    handle_disc(data.clone(), &builder);

    handle_scan(data.clone(), &builder, &window_clone);

    let config_button: Button = builder
        .object("config_button")
        .expect("Failed to get widget");
    handle_config(&config_button, &window_clone);

    let stop_button: Button = builder.object("stop_button").expect("Failed to get widget");
    stop_button.set_sensitive(false);
    handle_stop(ripping.clone(), &builder);

    handle_go(ripping, data, &builder);
}

fn handle_config(config_button: &Button, window: &ApplicationWindow) {
    let window = window.clone();
    config_button.connect_clicked(move |_| {
        let cfg: Config = confy::load("ripperx4", None).expect("Failed to load config");
        let config = Arc::new(RwLock::new(cfg));
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
        let options = ["mp3", "ogg", "flac", "opus"];
        let combo = DropDown::from_strings(&options);
        if let Ok(c) = config.read() {
            path.buffer().set_text(&c.encode_path);
            child.append(&path);
            let selected = match c.encoder {
                Encoder::MP3 => 0,
                Encoder::OGG => 1,
                Encoder::FLAC => 2,
                Encoder::OPUS => 3,
            };
            combo.set_selected(selected);
        } else {
            debug!("Failed to read config");
        }
        child.append(&combo);
        // quality
        let quality_options = ["low", "medium", "high"];
        let quality_combo = DropDown::from_strings(&quality_options);
        if let Ok(c) = config.read() {
            path.buffer().set_text(&c.encode_path);
            child.append(&path);
            let selected = match c.quality {
                Quality::Low => 0,
                Quality::Medium => 1,
                Quality::High => 2,
            };
            quality_combo.set_selected(selected);
        } else {
            debug!("Failed to read config");
        }
        child.append(&quality_combo);

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
            .transient_for(&window)
            .build();
        ok_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
            let buf = path.buffer();
            let new_path = path
                .buffer()
                .text(&buf.start_iter(), &buf.end_iter(), false);
            if let Ok(mut config) = config.write() {
                config.encode_path = new_path.to_string();
                let c = combo.selected();
                config.encoder = match c {
                    0 => Encoder::MP3,
                    1 => Encoder::OGG,
                    2 => Encoder::FLAC,
                    3 => Encoder::OPUS,
                    _ => panic!("invalid value"),
                };
                let c = quality_combo.selected();
                config.quality = match c {
                    0 => Quality::Low,
                    1 => Quality::Medium,
                    2 => Quality::High,
                    _ => panic!("invalid value"),
                };
                confy::store("ripperx4", None, &*config).ok();
            } else {
                debug!("Failed to write config");
            }
            dialog.close();
        }));
        cancel_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
            dialog.close();
        }));
        dialog.show();
    });
}

fn handle_disc(data: Arc<RwLock<Data>>, builder: &Builder) {
    let title_text: TextView = builder.object("disc_title").expect("Failed to get widget");
    let artist_text: TextView = builder.object("disc_artist").expect("Failed to get widget");
    let title_buffer = title_text.buffer();
    let data_title = data.clone();
    title_buffer.connect_changed(move |s| {
        if let Ok(mut data) = data_title.write() {
            if data.disc.is_some() {
                let new_title = s.text(&s.start_iter(), &s.end_iter(), false);
                if let Some(disc) = data.disc.as_mut() {
                    disc.title = new_title.to_string();
                }
            }
        }
    });
    let artist_buffer = artist_text.buffer();
    let data_artist = data;
    artist_buffer.connect_changed(move |s| {
        if let Ok(mut data) = data_artist.write() {
            if data.disc.is_some() {
                let new_artist = s.text(&s.start_iter(), &s.end_iter(), false);
                if let Some(disc) = data.disc.as_mut() {
                    disc.artist = new_artist.to_string();
                }
            }
        }
    });
}

fn handle_stop(ripping: Arc<RwLock<bool>>, builder: &Builder) {
    let builder = builder.clone();
    let stop_button: Button = builder.object("stop_button").expect("Failed to get widget");
    stop_button.connect_clicked(move |_| {
        debug!("stop");
        if let Ok(mut ripping) = ripping.write() {
            *ripping = false;
            let stop_button: Button = builder.object("stop_button").expect("Failed to get widget");
            stop_button.set_sensitive(false);
            let go_button: Button = builder.object("go_button").expect("Failed to get widget");
            go_button.set_sensitive(true); //
            let scan_button: Button = builder.object("scan_button").expect("Failed to get widget");
            scan_button.set_sensitive(true);
        }
    });
}

fn handle_scan(data: Arc<RwLock<Data>>, builder: &Builder, window: &ApplicationWindow) {
    let window = window.clone();
    let title_text: TextView = builder.object("disc_title").expect("Failed to get widget");
    let artist_text: TextView = builder.object("disc_artist").expect("Failed to get widget");
    let year_text: TextView = builder.object("year").expect("Failed to get widget");
    let genre_text: TextView = builder.object("genre").expect("Failed to get widget");
    let go_button: Button = builder.object("go_button").expect("Failed to get widget");
    let scroll: Box = builder.object("scroll").expect("Failed to get widget");
    let scan_button: Button = builder.object("scan_button").expect("Failed to get widget");
    scan_button.connect_clicked(move |_| {
        debug!("Scan");
        let result = DiscId::read(Some(&DiscId::default_device()));
        let discid = if let Ok(d) = result {
            d
        } else {
            // show_message("Disc not found!", MessageType::Error, &window);
            // return;
            // for testing on machine without CDROM drive: hardcode offsets of a dire straits disc
            let offsets = [
                298_948, 183, 26155, 44233, 64778, 80595, 117_410, 144_120, 159_913, 178_520,
                204_803, 258_763, 277_218,
            ];
            DiscId::put(1, &offsets).unwrap() // this is for testing only so this unwrap is ok
        };

        debug!("Scanned: {discid:?}");
        debug!("id={}", discid.id());
        if let Ok(disc) = crate::musicbrainz::lookup(&discid.id()) {
            debug!("disc:{}", disc.title);
            title_text.buffer().set_text(&disc.title);
            artist_text.buffer().set_text(&disc.artist);
            if let Some(year) = disc.year {
                year_text.buffer().set_text(&(year.to_string()));
            }
            if let Some(genre) = &disc.genre {
                genre_text.buffer().set_text(&genre.clone());
            }
            // panic if we can't get a write lock
            data.write()
                .expect("Failed to aquire write lock on data")
                .disc = Some(disc);
            // here we know how many tracks there are
            let tracks = usize::try_from(discid.last_track_num() - discid.first_track_num() + 1).expect("Failed to convert track number");
            for i in 0..tracks  {
                let hbox = Box::builder()
                    .orientation(Orientation::Horizontal)
                    .vexpand(false)
                    .hexpand(true)
                    .spacing(50)
                    .build();
                let label_text = format!("Track {}", i + 1);
                let label = Label::builder().label(&label_text).build();
                hbox.append(&label);

                if let Ok(r) = data.read() {
                    if let Some(d) = r.disc.as_ref() {
                        let title = &d.tracks[i].title;
                        let buffer = TextBuffer::builder().text(title).build();
                        let name = format!("{i}");
                        let tb = TextView::builder()
                            .name(&name)
                            .buffer(&buffer)
                            .hexpand(true)
                            .build();
                        let data_changed = data.clone();
                        buffer.connect_changed(glib::clone!(@weak buffer => move |_| {
                            if let Ok(mut r) = data_changed.write() {
                                if let Some(d) = r.disc.as_mut() {
                                    let tracks = &mut d.tracks;
                                    let track = &mut tracks[i];
                                    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                                    debug!("{}", &text);
                                    track.title = text.to_string();
                                    debug!("{}", &track.title);
                                }
                            }
                        }));
                        hbox.append(&tb);
                        tb.show();
                        scroll.append(&hbox);
                        hbox.show();
                    }
                }
            }
            scroll.show();
        } else {
            show_message("Disc not found!", MessageType::Error, &window);
        }
        go_button.set_sensitive(true);
    });
}

fn show_message(message: &str, typ: MessageType, window: &ApplicationWindow) {
    let dialog = MessageDialog::builder()
        .title("Error")
        .modal(true)
        .buttons(ButtonsType::Ok)
        .message_type(typ)
        .text(message)
        .transient_for(window)
        .width_request(300)
        .build();
    dialog.connect_response(glib::clone!(@weak dialog => move |_, _| {
        dialog.close();
    }));
    dialog.show();
}

fn handle_go(ripping_arc: Arc<RwLock<bool>>, data: Arc<RwLock<Data>>, builder: &Builder) {
    let builder = builder.clone();
    let go_button: Button = builder.object("go_button").expect("Failed to get widget");
    go_button.set_sensitive(false);
    let status: Statusbar = builder.object("statusbar").expect("Failed to get widget");
    let stop_button: Button = builder.object("stop_button").expect("Failed to get widget");
    go_button.connect_clicked(glib::clone!(@weak status => move |_| {
        if let Ok(mut ripping) = ripping_arc.write() {
            stop_button.set_sensitive(true);
            let go_button: Button = builder.object("go_button").expect("Failed to get widget");
            go_button.set_sensitive(false);
            let scan_button: Button = builder.object("scan_button").expect("Failed to get widget");
            scan_button.set_sensitive(false);
            *ripping = true;
            let context_id = status.context_id("foo");
            let (tx, rx) = async_channel::unbounded();
            let ripping_clone3 = ripping_arc.clone();
            thread::spawn(glib::clone!(@weak data => move || {
                if let Ok(data_go) = data.clone().read() {
                    if let Some(disc) = &data_go.disc {
                        match extract(disc, &tx, &ripping_clone3) {
                            Ok(_) => {
                                debug!("done");
                                let _ignore = tx.send("done".to_owned());
                            }
                            Err(e) => {
                                let msg = format!("Error: {e}");
                                debug!("{msg}");
                                let _ignore = tx.send(msg);
                            }
                        }
                    }
                }
            }));
            let scan_button_clone = scan_button;
            let go_button_clone = go_button;
            let stop_button_clone = stop_button.clone();
            glib::spawn_future_local(async move {
                while let Ok(value) =rx.recv().await {
                    let s = value.clone();
                    status.remove_all(context_id);
                    status.push(context_id, &s);
                    if s == "aborted" {
                        scan_button_clone.set_sensitive(true);
                        go_button_clone.set_sensitive(true);
                        stop_button_clone.set_sensitive(false);
                        break;
                    }
                    if s == "done" {
                        scan_button_clone.set_sensitive(true);
                        go_button_clone.set_sensitive(true);
                        stop_button_clone.set_sensitive(false);
                        break;
                    }
                }
            });
        }
    }));
}
