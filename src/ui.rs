use crate::{
    data::{Config, Data, Encoder, Quality},
    ripper::extract,
    util::{lookup_disc, scan_disc},
};
use glib::Type;
use gtk::{
    Align, Application, ApplicationWindow, Box, Builder, Button, ButtonsType, Dialog, DropDown,
    Frame, ListStore, MessageDialog, MessageType, Orientation, Picture, Separator, Statusbar,
    TextView, TreeView, prelude::*,
};
use log::debug;
use std::{
    sync::{Arc, RwLock},
    thread,
};

pub fn build(app: &Application) {
    let data = Arc::new(RwLock::new(Data {
        ..Default::default()
    }));
    let ripping = Arc::new(RwLock::new(false));

    let builder = Builder::new();
    builder
        .add_from_resource("/ripperx4.ui")
        .expect("failed to load UI");
    set_picture(&builder, "logo_picture", "/images/ripperX.png");
    set_picture(&builder, "config_picture", "/images/config.png");
    set_picture(&builder, "scan_picture", "/images/scan.png");
    set_picture(&builder, "stop_picture", "/images/stop.png");
    set_picture(&builder, "go_picture", "/images/go.png");
    set_picture(&builder, "exit_picture", "/images/exit.png");

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

fn set_picture(builder: &Builder, id: &str, resource: &str) {
    let picture: Picture = builder
        .object(id)
        .unwrap_or_else(|| panic!("Failed to get picture {id}"));
    picture.set_resource(Some(resource));
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
        ok_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| {
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
            }
        ));
        cancel_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| {
                dialog.close();
            }
        ));
        dialog.show();
    });
}

fn handle_disc(data: Arc<RwLock<Data>>, builder: &Builder) {
    let title_text: TextView = builder.object("disc_title").expect("Failed to get widget");
    let artist_text: TextView = builder.object("disc_artist").expect("Failed to get widget");
    let title_buffer = title_text.buffer();
    let data_title = data.clone();
    title_buffer.connect_changed(move |s| {
        if let Ok(mut data) = data_title.write()
            && data.disc.is_some()
        {
            let new_title = s.text(&s.start_iter(), &s.end_iter(), false);
            if let Some(disc) = data.disc.as_mut() {
                disc.title = new_title.to_string();
            }
        }
    });
    let artist_buffer = artist_text.buffer();
    let data_artist = data;
    artist_buffer.connect_changed(move |s| {
        if let Ok(mut data) = data_artist.write()
            && data.disc.is_some()
        {
            let new_artist = s.text(&s.start_iter(), &s.end_iter(), false);
            if let Some(disc) = data.disc.as_mut() {
                disc.artist = new_artist.to_string();
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
    // build treeview
    let tree: TreeView = builder
        .object("track_listview")
        .expect("Failed to get widget");
    let store = ListStore::new(&[Type::BOOL, Type::U32, Type::STRING, Type::STRING]);
    tree.set_model(Some(&store));
    let bool_renderer = gtk::CellRendererToggle::new();
    bool_renderer.set_property("activatable", true);
    let t = tree.clone();
    let m = t.model().expect("Failed to get model");
    let s = store.clone();
    let d_clone = data.clone();
    bool_renderer.connect_toggled(move |_, path| {
        let iter = m.iter(&path).expect("Failed to get iter");
        let old = s
            .get_value(&iter, 0)
            .get::<bool>()
            .expect("Failed to get value");
        let new = !old;
        s.set_value(&iter, 0, &new.to_value());
        if let Some(d) = d_clone
            .write()
            .expect("Failed to aquire write lock on data")
            .disc
            .as_mut()
        {
            let num = m
                .get_value(&iter, 1)
                .get::<u8>()
                .expect("Failed to get value");
            d.tracks[num as usize - 1].rip = new;
        }
    });
    let column = gtk::TreeViewColumn::with_attributes("Encode", &bool_renderer, &[("active", 0)]);
    tree.append_column(&column);

    let renderer = gtk::CellRendererText::new();
    let column = gtk::TreeViewColumn::with_attributes("Track", &renderer, &[("text", 1)]);
    tree.append_column(&column);

    let renderer = gtk::CellRendererText::new();
    renderer.set_property("editable", true);
    let t = tree.clone();
    let m = t.model().expect("Failed to get model");
    let s = store.clone();
    let d_clone = data.clone();
    renderer.connect_edited(move |_, path, new_text| {
        let iter = m.iter(&path).expect("Failed to get iter");
        s.set_value(&iter, 2, &new_text.to_value());
        if let Some(d) = d_clone
            .write()
            .expect("Failed to aquire write lock on data")
            .disc
            .as_mut()
        {
            let num = m
                .get_value(&iter, 1)
                .get::<u8>()
                .expect("Failed to get value");
            d.tracks[num as usize - 1].title = new_text.to_string();
        };
    });
    let column = gtk::TreeViewColumn::with_attributes("Title", &renderer, &[("text", 2)]);
    tree.append_column(&column);

    let renderer = gtk::CellRendererText::new();
    renderer.set_property("editable", true);
    let t = tree.clone();
    let m = t.model().expect("Failed to get model");
    let s = store.clone();
    let d_clone = data.clone();
    renderer.connect_edited(move |_, path, new_text| {
        let iter = m.iter(&path).expect("Failed to get iter");
        s.set_value(&iter, 3, &new_text.to_value());
        if let Some(d) = d_clone
            .write()
            .expect("Failed to aquire write lock on data")
            .disc
            .as_mut()
        {
            let num = m
                .get_value(&iter, 1)
                .get::<u8>()
                .expect("Failed to get value");
            d.tracks[num as usize - 1].artist = new_text.to_string();
        };
    });
    let column = gtk::TreeViewColumn::with_attributes("Artist", &renderer, &[("text", 3)]);
    tree.append_column(&column);

    let scan_button: Button = builder.object("scan_button").expect("Failed to get widget");
    scan_button.connect_clicked(move |_| {
        debug!("Scan");
        if let Ok(discid) = scan_disc() {
            debug!("Scanned: {discid:?}");
            debug!("id={}", discid.id());
            let disc = lookup_disc(&discid);
            debug!("disc:{}", disc.title);
            // store.clear();
            title_text.buffer().set_text(&disc.title);
            artist_text.buffer().set_text(&disc.artist);
            if let Some(year) = disc.year {
                year_text.buffer().set_text(&(year.to_string()));
            }
            if let Some(genre) = &disc.genre {
                genre_text.buffer().set_text(&genre.clone());
            }
            let tracks = disc.tracks.len();
            // panic if we can't get a write lock
            data.write()
                .expect("Failed to aquire write lock on data")
                .disc = Some(disc);
            // here we know how many tracks there are
            for i in 0..tracks {
                let iter = store.append();
                if let Ok(r) = data.read()
                    && let Some(d) = r.disc.as_ref()
                {
                    let num = d.tracks[i].number;
                    let title = &d.tracks[i].title.clone();
                    let artist = &d.tracks[i].artist.clone();
                    debug!("{}: {} - {}", num, title, artist);
                    store.set(&iter, &[(0, &true), (1, &num), (2, &title), (3, &artist)]);
                }
            }
            go_button.set_sensitive(true);
        } else {
            show_message("Failed to scan disc", MessageType::Error, &window);
        }
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
    dialog.connect_response(glib::clone!(
        #[weak]
        dialog,
        move |_, _| {
            dialog.close();
        }
    ));
    dialog.show();
}

fn handle_go(ripping_arc: Arc<RwLock<bool>>, data: Arc<RwLock<Data>>, builder: &Builder) {
    let builder = builder.clone();
    let go_button: Button = builder.object("go_button").expect("Failed to get widget");
    go_button.set_sensitive(false);
    let status: Statusbar = builder.object("statusbar").expect("Failed to get widget");
    let stop_button: Button = builder.object("stop_button").expect("Failed to get widget");
    go_button.connect_clicked(glib::clone!(
        #[weak]
        status,
        move |_| {
            if let Ok(mut ripping) = ripping_arc.write() {
                stop_button.set_sensitive(true);
                let go_button: Button = builder.object("go_button").expect("Failed to get widget");
                go_button.set_sensitive(false);
                let scan_button: Button =
                    builder.object("scan_button").expect("Failed to get widget");
                scan_button.set_sensitive(false);
                *ripping = true;
                let context_id = status.context_id("foo");
                let (tx, rx) = async_channel::unbounded();
                let ripping_clone3 = ripping_arc.clone();
                thread::spawn(glib::clone!(
                    #[weak]
                    data,
                    move || {
                        if let Ok(data_go) = data.clone().read()
                            && let Some(disc) = &data_go.disc
                        {
                            match extract(disc, &tx, &ripping_clone3) {
                                Ok(()) => {
                                    debug!("done");
                                    tx.send_blocking("done".to_owned()).ok();
                                }
                                Err(e) => {
                                    let msg = format!("Error: {e}");
                                    debug!("{msg}");
                                    tx.send_blocking("aborted".to_owned()).ok();
                                }
                            }
                        }
                    }
                ));
                let scan_button_clone = scan_button;
                let go_button_clone = go_button;
                let stop_button_clone = stop_button.clone();
                glib::spawn_future_local(async move {
                    while let Ok(value) = rx.recv().await {
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
        }
    ));
}
