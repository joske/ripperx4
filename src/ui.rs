use crate::{
    data::{Config, Data, Encoder, Quality},
    ripper::extract,
    util::{lookup_disc, read_config, scan_disc, write_config},
};
use glib::{Type, prelude::IsA};
use gtk::{
    Align, Application, ApplicationWindow, Box, Builder, Button, ButtonsType, Dialog, DropDown,
    Entry, Frame, ListStore, MessageDialog, MessageType, Orientation, Picture, Separator,
    Statusbar, TreeView, prelude::*,
};
use log::{debug, warn};
use std::{
    sync::{Arc, RwLock},
    thread,
};

/// Helper to get a widget from builder, logging errors instead of panicking
fn get_widget<T: IsA<glib::Object>>(builder: &Builder, id: &str) -> Option<T> {
    builder.object(id).or_else(|| {
        warn!("Failed to get widget: {id}");
        None
    })
}

/// Set a picture resource, logging if the widget is not found
fn set_picture(builder: &Builder, id: &str, resource: &str) {
    if let Some(picture) = get_widget::<Picture>(builder, id) {
        picture.set_resource(Some(resource));
    }
}

/// Wrapper for buttons that manages state together
struct ButtonGroup {
    scan: Button,
    stop: Button,
    go: Button,
}

impl ButtonGroup {
    fn from_builder(builder: &Builder) -> Option<Self> {
        Some(Self {
            scan: get_widget(builder, "scan_button")?,
            stop: get_widget(builder, "stop_button")?,
            go: get_widget(builder, "go_button")?,
        })
    }

    fn set_ripping(&self, ripping: bool) {
        self.scan.set_sensitive(!ripping);
        self.stop.set_sensitive(ripping);
        self.go.set_sensitive(!ripping);
    }

    fn set_idle(&self, has_disc: bool) {
        self.scan.set_sensitive(true);
        self.stop.set_sensitive(false);
        self.go.set_sensitive(has_disc);
    }
}

pub fn build(app: &Application) {
    let data = Arc::new(RwLock::new(Data::default()));
    let ripping = Arc::new(RwLock::new(false));

    let builder = Builder::new();
    if let Err(e) = builder.add_from_resource("/ripperx4.ui") {
        warn!("Failed to load UI: {e}");
        return;
    }

    // Set up pictures
    set_picture(&builder, "logo_picture", "/images/ripperX.png");
    set_picture(&builder, "config_picture", "/images/config.png");
    set_picture(&builder, "scan_picture", "/images/scan.png");
    set_picture(&builder, "stop_picture", "/images/stop.png");
    set_picture(&builder, "go_picture", "/images/go.png");
    set_picture(&builder, "exit_picture", "/images/exit.png");

    let Some(window) = get_widget::<ApplicationWindow>(&builder, "window") else {
        warn!("Failed to get main window");
        return;
    };
    window.set_application(Some(app));
    window.present();

    // Exit button
    if let Some(exit_button) = get_widget::<Button>(&builder, "exit") {
        let w = window.clone();
        exit_button.connect_clicked(move |_| w.close());
    }

    // Initialize button states
    if let Some(stop_button) = get_widget::<Button>(&builder, "stop_button") {
        stop_button.set_sensitive(false);
    }
    if let Some(go_button) = get_widget::<Button>(&builder, "go_button") {
        go_button.set_sensitive(false);
    }

    handle_disc(data.clone(), &builder);
    handle_scan(data.clone(), &builder, &window);
    handle_config(&builder, &window);
    handle_stop(ripping.clone(), data.clone(), &builder);
    handle_go(ripping, data, &builder);
}

fn handle_config(builder: &Builder, window: &ApplicationWindow) {
    let Some(config_button) = get_widget::<Button>(builder, "config_button") else {
        return;
    };
    let window = window.clone();

    config_button.connect_clicked(move |_| {
        let cfg: Config = read_config();
        let config = Arc::new(RwLock::new(cfg));

        let child = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_start(10)
            .margin_end(10)
            .margin_top(10)
            .margin_bottom(10)
            .hexpand(true)
            .vexpand(true)
            .build();

        let frame = Frame::builder()
            .child(&child)
            .label("Configuration")
            .hexpand(true)
            .vexpand(true)
            .build();

        // Path entry
        let path_entry = Entry::builder()
            .hexpand(true)
            .placeholder_text("Output path")
            .build();

        // Encoder dropdown
        let encoder_combo = DropDown::from_strings(Encoder::OPTIONS);

        // Quality dropdown
        let quality_combo = DropDown::from_strings(Quality::OPTIONS);

        // Populate from config
        if let Ok(c) = config.read() {
            path_entry.set_text(&c.encode_path);
            encoder_combo.set_selected(c.encoder.to_index());
            quality_combo.set_selected(c.quality.to_index());
        }

        // Path label and entry
        let path_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        path_box.append(&gtk::Label::new(Some("Output path:")));
        path_box.append(&path_entry);
        child.append(&path_box);

        // Encoder label and combo
        let encoder_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        encoder_box.append(&gtk::Label::new(Some("Encoder:")));
        encoder_box.append(&encoder_combo);
        child.append(&encoder_box);

        // Quality label and combo
        let quality_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        quality_box.append(&gtk::Label::new(Some("Quality:")));
        quality_box.append(&quality_combo);
        child.append(&quality_box);

        let separator = Separator::builder().vexpand(true).build();
        child.append(&separator);

        let button_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .halign(Align::End)
            .build();
        let ok_button = Button::builder().label("Ok").build();
        let cancel_button = Button::builder().label("Cancel").build();
        button_box.append(&ok_button);
        button_box.append(&cancel_button);
        child.append(&button_box);

        let dialog = Dialog::builder()
            .title("Configuration")
            .modal(true)
            .child(&frame)
            .width_request(400)
            .transient_for(&window)
            .build();

        ok_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| {
                if let Ok(mut cfg) = config.write() {
                    cfg.encode_path = path_entry.text().to_string();
                    cfg.encoder = Encoder::from_index(encoder_combo.selected());
                    cfg.quality = Quality::from_index(quality_combo.selected());
                    write_config(&cfg);
                }
                dialog.close();
            }
        ));

        cancel_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| dialog.close()
        ));

        dialog.show();
    });
}

fn handle_disc(data: Arc<RwLock<Data>>, builder: &Builder) {
    let Some(title_entry) = get_widget::<Entry>(builder, "disc_title") else {
        return;
    };
    let Some(artist_entry) = get_widget::<Entry>(builder, "disc_artist") else {
        return;
    };
    let Some(year_entry) = get_widget::<Entry>(builder, "year") else {
        return;
    };
    let Some(genre_entry) = get_widget::<Entry>(builder, "genre") else {
        return;
    };

    let data_title = data.clone();
    title_entry.connect_changed(move |entry| {
        if let Ok(mut data) = data_title.write()
            && let Some(disc) = data.disc.as_mut()
        {
            disc.title = entry.text().to_string();
        }
    });

    let data_artist = data.clone();
    artist_entry.connect_changed(move |entry| {
        if let Ok(mut data) = data_artist.write()
            && let Some(disc) = data.disc.as_mut()
        {
            disc.artist = entry.text().to_string();
        }
    });

    let data_year = data.clone();
    year_entry.connect_changed(move |entry| {
        if let Ok(mut data) = data_year.write()
            && let Some(disc) = data.disc.as_mut()
        {
            disc.year = entry.text().parse::<u16>().ok();
        }
    });

    genre_entry.connect_changed(move |entry| {
        if let Ok(mut data) = data.write()
            && let Some(disc) = data.disc.as_mut()
        {
            let text = entry.text().to_string();
            disc.genre = if text.is_empty() { None } else { Some(text) };
        }
    });
}

fn handle_stop(ripping: Arc<RwLock<bool>>, data: Arc<RwLock<Data>>, builder: &Builder) {
    let Some(buttons) = ButtonGroup::from_builder(builder) else {
        return;
    };

    let stop = buttons.stop.clone();
    stop.connect_clicked(move |_| {
        debug!("stop");
        if let Ok(mut r) = ripping.write() {
            *r = false;
            let has_disc = data.read().ok().is_some_and(|d| d.disc.is_some());
            buttons.set_idle(has_disc);
        }
    });
}

#[allow(clippy::too_many_lines)] // GTK handler with multiple widget setups
fn handle_scan(data: Arc<RwLock<Data>>, builder: &Builder, window: &ApplicationWindow) {
    let window = window.clone();

    let Some(title_entry) = get_widget::<Entry>(builder, "disc_title") else {
        return;
    };
    let Some(artist_entry) = get_widget::<Entry>(builder, "disc_artist") else {
        return;
    };
    let Some(year_entry) = get_widget::<Entry>(builder, "year") else {
        return;
    };
    let Some(genre_entry) = get_widget::<Entry>(builder, "genre") else {
        return;
    };
    let Some(go_button) = get_widget::<Button>(builder, "go_button") else {
        return;
    };
    let Some(tree) = get_widget::<TreeView>(builder, "track_listview") else {
        return;
    };
    let Some(scan_button) = get_widget::<Button>(builder, "scan_button") else {
        return;
    };

    // Build tree model
    let store = ListStore::new(&[Type::BOOL, Type::U32, Type::STRING, Type::STRING]);
    tree.set_model(Some(&store));

    // Encode column (toggle)
    let toggle_renderer = gtk::CellRendererToggle::new();
    toggle_renderer.set_property("activatable", true);
    {
        let model = store.clone();
        let data = data.clone();
        toggle_renderer.connect_toggled(move |_, path| {
            let Some(iter) = model.iter(&path) else {
                return;
            };
            let Ok(old) = model.get_value(&iter, 0).get::<bool>() else {
                return;
            };
            let new = !old;
            model.set_value(&iter, 0, &new.to_value());

            if let Ok(mut d) = data.write()
                && let Some(disc) = d.disc.as_mut()
                && let Ok(num) = model.get_value(&iter, 1).get::<u32>()
                && let Some(track) = disc.tracks.get_mut(num as usize - 1)
            {
                track.rip = new;
            }
        });
    }
    tree.append_column(&gtk::TreeViewColumn::with_attributes(
        "Encode",
        &toggle_renderer,
        &[("active", 0)],
    ));

    // Track number column
    tree.append_column(&gtk::TreeViewColumn::with_attributes(
        "Track",
        &gtk::CellRendererText::new(),
        &[("text", 1)],
    ));

    // Title column (editable)
    let title_renderer = gtk::CellRendererText::new();
    title_renderer.set_property("editable", true);
    {
        let model = store.clone();
        let data = data.clone();
        title_renderer.connect_edited(move |_, path, new_text| {
            let Some(iter) = model.iter(&path) else {
                return;
            };
            model.set_value(&iter, 2, &new_text.to_value());

            if let Ok(mut d) = data.write()
                && let Some(disc) = d.disc.as_mut()
                && let Ok(num) = model.get_value(&iter, 1).get::<u32>()
                && let Some(track) = disc.tracks.get_mut(num as usize - 1)
            {
                track.title = new_text.to_string();
            }
        });
    }
    tree.append_column(&gtk::TreeViewColumn::with_attributes(
        "Title",
        &title_renderer,
        &[("text", 2)],
    ));

    // Artist column (editable)
    let artist_renderer = gtk::CellRendererText::new();
    artist_renderer.set_property("editable", true);
    {
        let model = store.clone();
        let data = data.clone();
        artist_renderer.connect_edited(move |_, path, new_text| {
            let Some(iter) = model.iter(&path) else {
                return;
            };
            model.set_value(&iter, 3, &new_text.to_value());

            if let Ok(mut d) = data.write()
                && let Some(disc) = d.disc.as_mut()
                && let Ok(num) = model.get_value(&iter, 1).get::<u32>()
                && let Some(track) = disc.tracks.get_mut(num as usize - 1)
            {
                track.artist = new_text.to_string();
            }
        });
    }
    tree.append_column(&gtk::TreeViewColumn::with_attributes(
        "Artist",
        &artist_renderer,
        &[("text", 3)],
    ));

    // Scan button click handler
    scan_button.connect_clicked(move |_| {
        debug!("Scan");
        match scan_disc() {
            Ok(discid) => {
                debug!("Scanned: {discid:?}");
                debug!("id={}", discid.id());

                let disc = lookup_disc(&discid);
                debug!("disc: {}", disc.title);

                title_entry.set_text(&disc.title);
                artist_entry.set_text(&disc.artist);
                year_entry.set_text(&disc.year.map_or(String::new(), |y| y.to_string()));
                genre_entry.set_text(disc.genre.as_deref().unwrap_or(""));

                if let Ok(mut d) = data.write() {
                    d.disc = Some(disc);
                }

                store.clear();
                if let Ok(d) = data.read()
                    && let Some(disc) = &d.disc
                {
                    for track in &disc.tracks {
                        let iter = store.append();
                        store.set(
                            &iter,
                            &[
                                (0, &true),
                                (1, &track.number),
                                (2, &track.title),
                                (3, &track.artist),
                            ],
                        );
                    }
                }
                go_button.set_sensitive(true);
            }
            Err(e) => {
                debug!("Scan failed: {e}");
                show_message("Failed to scan disc", MessageType::Error, &window);
            }
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
        move |_, _| dialog.close()
    ));
    dialog.show();
}

fn handle_go(ripping_arc: Arc<RwLock<bool>>, data: Arc<RwLock<Data>>, builder: &Builder) {
    let Some(buttons) = ButtonGroup::from_builder(builder) else {
        return;
    };
    let Some(status) = get_widget::<Statusbar>(builder, "statusbar") else {
        return;
    };

    let go_button = buttons.go.clone();
    let stop_button = buttons.stop.clone();
    let scan_button = buttons.scan.clone();

    let go_clone = go_button.clone();
    go_clone.connect_clicked(glib::clone!(
        #[weak]
        status,
        move |_| {
            let Ok(mut ripping) = ripping_arc.write() else {
                return;
            };

            buttons.set_ripping(true);
            *ripping = true;

            let context_id = status.context_id("ripping");
            let (tx, rx) = async_channel::unbounded();
            let ripping_clone = ripping_arc.clone();

            thread::spawn(glib::clone!(
                #[weak]
                data,
                move || {
                    let result = data.read().ok().and_then(|d| {
                        d.disc
                            .as_ref()
                            .map(|disc| extract(disc, &tx, &ripping_clone))
                    });

                    match result {
                        Some(Ok(())) => {
                            debug!("done");
                            let _ = tx.send_blocking("done".to_owned());
                        }
                        Some(Err(e)) => {
                            debug!("Error: {e}");
                            let _ = tx.send_blocking("aborted".to_owned());
                        }
                        None => {
                            let _ = tx.send_blocking("aborted".to_owned());
                        }
                    }
                }
            ));

            let scan_btn = scan_button.clone();
            let go_btn = go_button.clone();
            let stop_btn = stop_button.clone();

            glib::spawn_future_local(async move {
                while let Ok(msg) = rx.recv().await {
                    status.remove_all(context_id);
                    status.push(context_id, &msg);

                    if msg == "done" || msg == "aborted" {
                        scan_btn.set_sensitive(true);
                        go_btn.set_sensitive(true);
                        stop_btn.set_sensitive(false);
                        break;
                    }
                }
            });
        }
    ));
}
