use crate::{
    data::{Config, Data, Encoder, Quality},
    ripper::{check_existing_files, create_playlist, extract},
    util::{lookup_disc, read_config, scan_disc, write_config},
};
use glib::{Type, prelude::IsA};
use gtk::{
    Align, Application, ApplicationWindow, Box, Builder, Button, ButtonsType, CheckButton, Dialog,
    DropDown, Entry, Frame, ListStore, MessageDialog, MessageType, Orientation, Picture, Separator,
    Statusbar, TreeView, gio, prelude::*,
};
use log::{debug, warn};
use std::{
    process::Command,
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

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    let secs = seconds % 60;
    format!("{minutes}:{secs:02}")
}

fn eject_cd() {
    let result = if cfg!(target_os = "macos") {
        Command::new("drutil").arg("eject").output()
    } else {
        Command::new("eject").output()
    };
    match result {
        Ok(_) => debug!("CDROM ejected"),
        Err(e) => warn!("Failed to eject CDROM: {e}"),
    }
}

/// Wrapper for buttons that manages state together
#[derive(Clone)]
struct ButtonGroup {
    scan: Button,
    stop: Button,
    go: Button,
    config: Button,
}

impl ButtonGroup {
    fn from_builder(builder: &Builder) -> Option<Self> {
        Some(Self {
            scan: get_widget(builder, "scan_button")?,
            stop: get_widget(builder, "stop_button")?,
            go: get_widget(builder, "go_button")?,
            config: get_widget(builder, "config_button")?,
        })
    }

    fn set_ripping(&self, ripping: bool) {
        self.scan.set_sensitive(!ripping);
        self.stop.set_sensitive(ripping);
        self.go.set_sensitive(!ripping);
        self.config.set_sensitive(!ripping);
    }

    fn set_idle(&self, has_disc: bool) {
        self.scan.set_sensitive(true);
        self.stop.set_sensitive(false);
        self.go.set_sensitive(has_disc);
        self.config.set_sensitive(true);
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
    if let Some(select_all_button) = get_widget::<Button>(&builder, "select_all_button") {
        select_all_button.set_sensitive(false);
    }

    handle_disc(data.clone(), &builder);
    handle_scan(&data.clone(), &builder, &window);
    handle_config(&builder, &window);
    handle_stop(ripping.clone(), data.clone(), &builder);
    handle_go(app, ripping, data, &builder, &window);
}

#[allow(clippy::too_many_lines)]
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
            quality_combo.set_sensitive(c.encoder.has_quality_setting());
        }

        // Disable quality dropdown when WAV is selected
        let quality_for_encoder = quality_combo.clone();
        encoder_combo.connect_selected_notify(move |combo| {
            let encoder = Encoder::from_index(combo.selected());
            quality_for_encoder.set_sensitive(encoder.has_quality_setting());
        });

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

        // Eject when done checkbox
        let eject_check = CheckButton::builder()
            .label("Eject CD when finished")
            .build();
        if let Ok(c) = config.read() {
            eject_check.set_active(c.eject_when_done);
        }
        child.append(&eject_check);

        // Create playlist checkbox
        let playlist_check = CheckButton::builder().label("Create M3U playlist").build();
        if let Ok(c) = config.read() {
            playlist_check.set_active(c.create_playlist);
        }
        child.append(&playlist_check);

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
                    cfg.eject_when_done = eject_check.is_active();
                    cfg.create_playlist = playlist_check.is_active();
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
    let Some(track_view) = get_widget::<TreeView>(builder, "track_listview") else {
        return;
    };

    let stop = buttons.stop.clone();
    stop.connect_clicked(glib::clone!(
        #[weak]
        track_view,
        move |_| {
            debug!("stop");
            if let Ok(mut r) = ripping.write() {
                *r = false;
                let has_disc = data.read().ok().is_some_and(|d| d.disc.is_some());
                buttons.set_idle(has_disc);
                track_view.set_sensitive(true);
            }
        }
    ));
}

#[allow(clippy::too_many_lines)] // GTK handler with multiple widget setups
fn handle_scan(data: &Arc<RwLock<Data>>, builder: &Builder, window: &ApplicationWindow) {
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
    let Some(stop_button) = get_widget::<Button>(builder, "stop_button") else {
        return;
    };
    let Some(tree) = get_widget::<TreeView>(builder, "track_listview") else {
        return;
    };
    let Some(scan_button) = get_widget::<Button>(builder, "scan_button") else {
        return;
    };
    let Some(config_button) = get_widget::<Button>(builder, "config_button") else {
        return;
    };
    let Some(exit_button) = get_widget::<Button>(builder, "exit") else {
        return;
    };

    // Build tree model
    let store = ListStore::new(&[
        Type::BOOL,
        Type::U32,
        Type::STRING,
        Type::STRING,
        Type::STRING,
    ]);
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

    tree.append_column(&gtk::TreeViewColumn::with_attributes(
        "Length",
        &gtk::CellRendererText::new(),
        &[("text", 4)],
    ));

    // Select All button handler
    if let Some(select_all_button) = get_widget::<Button>(builder, "select_all_button") {
        let store_for_select = store.clone();
        let data_for_select = data.clone();
        select_all_button.connect_clicked(move |_| {
            // Determine if we should select or deselect all
            let mut should_select = false;
            if let Some(iter) = store_for_select.iter_first() {
                loop {
                    if let Ok(checked) = store_for_select.get_value(&iter, 0).get::<bool>()
                        && !checked
                    {
                        should_select = true;
                        break;
                    }
                    if !store_for_select.iter_next(&iter) {
                        break;
                    }
                }
            }

            // Update data model
            if let Ok(mut d) = data_for_select.write()
                && let Some(disc) = d.disc.as_mut()
            {
                for track in &mut disc.tracks {
                    track.rip = should_select;
                }
            }

            // Update UI
            if let Some(iter) = store_for_select.iter_first() {
                loop {
                    store_for_select.set_value(&iter, 0, &should_select.to_value());
                    if !store_for_select.iter_next(&iter) {
                        break;
                    }
                }
            }
        });
    }

    // Eject button handler
    if let Some(eject_button) = get_widget::<Button>(builder, "eject_button") {
        eject_button.connect_clicked(move |_| {
            eject_cd();
        });
    }

    // Scan button click handler
    let scan_button_clone = scan_button.clone();
    let go_button_clone = go_button.clone();
    let stop_button_clone = stop_button.clone();
    let config_button_clone = config_button.clone();
    let exit_button_clone = exit_button.clone();
    let select_all_button_clone = get_widget::<Button>(builder, "select_all_button");
    let store_clone = store.clone();
    let data_clone = data.clone();
    let title_entry_clone = title_entry.clone();
    let artist_entry_clone = artist_entry.clone();
    let year_entry_clone = year_entry.clone();
    let genre_entry_clone = genre_entry.clone();
    let window_clone = window.clone();

    scan_button.connect_clicked(move |_| {
        debug!("Scan");

        let mut button_states: Vec<(Button, bool)> = Vec::new();
        for button in [
            scan_button_clone.clone(),
            go_button_clone.clone(),
            stop_button_clone.clone(),
            config_button_clone.clone(),
            exit_button_clone.clone(),
        ] {
            let was_sensitive = button.is_sensitive();
            button.set_sensitive(false);
            button_states.push((button, was_sensitive));
        }

        let (tx, rx) = async_channel::bounded(1);

        thread::spawn({
            let tx = tx.clone();
            move || {
                let result = match scan_disc() {
                    Ok(discid) => {
                        debug!("Scanned: {discid:?}");
                        debug!("id={}", discid.id());
                        let disc = lookup_disc(&discid);
                        debug!("disc: {}", disc.title);
                        Ok(disc)
                    }
                    Err(e) => Err(format!("{e}")),
                };
                let _ = tx.send_blocking(result);
            }
        });

        let data_for_handler = data_clone.clone();
        let store_for_handler = store_clone.clone();
        let title_entry_for_handler = title_entry_clone.clone();
        let artist_entry_for_handler = artist_entry_clone.clone();
        let year_entry_for_handler = year_entry_clone.clone();
        let genre_entry_for_handler = genre_entry_clone.clone();
        let window_for_handler = window_clone.clone();
        let go_button_for_handler = go_button_clone.clone();
        let select_all_for_handler = select_all_button_clone.clone();
        let button_states_for_handler = button_states;

        glib::spawn_future_local(async move {
            let recv_result = rx.recv().await;

            for (button, was_sensitive) in &button_states_for_handler {
                button.set_sensitive(*was_sensitive);
            }

            match recv_result {
                Ok(Ok(disc)) => {
                    let year_text = disc.year.map_or(String::new(), |y| y.to_string());
                    let genre_text = disc.genre.clone().unwrap_or_default();

                    title_entry_for_handler.set_text(&disc.title);
                    artist_entry_for_handler.set_text(&disc.artist);
                    year_entry_for_handler.set_text(&year_text);
                    genre_entry_for_handler.set_text(&genre_text);

                    store_for_handler.clear();
                    for track in &disc.tracks {
                        let iter = store_for_handler.append();
                        let duration = format_duration(track.duration);
                        store_for_handler.set(
                            &iter,
                            &[
                                (0, &track.rip),
                                (1, &track.number),
                                (2, &track.title),
                                (3, &track.artist),
                                (4, &duration),
                            ],
                        );
                    }

                    if let Ok(mut d) = data_for_handler.write() {
                        d.disc = Some(disc);
                    }

                    go_button_for_handler.set_sensitive(true);

                    if let Some(btn) = &select_all_for_handler {
                        btn.set_sensitive(true);
                    }
                }
                Ok(Err(err)) => {
                    debug!("Scan failed: {err}");
                    show_message(
                        "Failed to scan disc",
                        MessageType::Error,
                        &window_for_handler,
                    );
                }
                Err(err) => {
                    debug!("Scan channel closed unexpectedly: {err}");
                    show_message(
                        "Failed to scan disc",
                        MessageType::Error,
                        &window_for_handler,
                    );
                }
            }
        });
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

fn handle_go(
    app: &Application,
    ripping_arc: Arc<RwLock<bool>>,
    data: Arc<RwLock<Data>>,
    builder: &Builder,
    window: &ApplicationWindow,
) {
    let Some(buttons) = ButtonGroup::from_builder(builder) else {
        return;
    };
    let Some(status) = get_widget::<Statusbar>(builder, "statusbar") else {
        return;
    };
    let Some(track_view) = get_widget::<TreeView>(builder, "track_listview") else {
        return;
    };

    let go_button = buttons.go.clone();
    let stop_button = buttons.stop.clone();
    let scan_button = buttons.scan.clone();
    let config_button = buttons.config.clone();
    let data_for_notify = data.clone();
    let window = window.clone();

    let go_clone = go_button.clone();
    go_clone.connect_clicked(glib::clone!(
        #[weak]
        status,
        #[strong]
        app,
        #[weak]
        track_view,
        move |_| {
            // Check for existing files first
            let existing_files = data
                .read()
                .ok()
                .and_then(|d| d.disc.as_ref().map(check_existing_files))
                .unwrap_or_default();

            let do_rip = {
                let buttons = buttons.clone();
                let ripping_arc = ripping_arc.clone();
                let status = status.clone();
                let track_view = track_view.clone();
                let data = data.clone();
                let app = app.clone();
                let data_for_notify = data_for_notify.clone();
                let go_button = go_button.clone();
                let stop_button = stop_button.clone();
                let scan_button = scan_button.clone();
                let config_button = config_button.clone();
                move |overwrite: bool| {
                    start_ripping(
                        overwrite,
                        &buttons,
                        &ripping_arc,
                        &status,
                        &track_view,
                        &data,
                        &app,
                        &data_for_notify,
                        &go_button,
                        &stop_button,
                        &scan_button,
                        &config_button,
                    );
                }
            };

            if existing_files.is_empty() {
                do_rip(false);
            } else {
                show_overwrite_dialog(&existing_files, &window, do_rip);
            }
        }
    ));
}

fn show_overwrite_dialog(
    existing_files: &[String],
    window: &ApplicationWindow,
    on_overwrite: impl Fn(bool) + 'static,
) {
    let count = existing_files.len();
    let message = if count == 1 {
        format!(
            "1 file already exists:\n{}\n\nWhat would you like to do?",
            existing_files[0]
        )
    } else {
        format!("{count} files already exist. What would you like to do?")
    };

    let dialog = MessageDialog::builder()
        .title("Files Exist")
        .modal(true)
        .message_type(MessageType::Question)
        .text(&message)
        .transient_for(window)
        .build();

    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Overwrite", gtk::ResponseType::Accept);

    dialog.connect_response(move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Accept {
            on_overwrite(true);
        }
    });

    dialog.show();
}

#[allow(clippy::too_many_arguments)]
fn start_ripping(
    overwrite: bool,
    buttons: &ButtonGroup,
    ripping_arc: &Arc<RwLock<bool>>,
    status: &Statusbar,
    track_view: &TreeView,
    data: &Arc<RwLock<Data>>,
    app: &Application,
    data_for_notify: &Arc<RwLock<Data>>,
    go_button: &Button,
    stop_button: &Button,
    scan_button: &Button,
    config_button: &Button,
) {
    let Ok(mut ripping) = ripping_arc.write() else {
        return;
    };

    buttons.set_ripping(true);
    *ripping = true;
    track_view.set_sensitive(false);

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
                    .map(|disc| extract(disc, &tx, &ripping_clone, overwrite))
            });

            match result {
                Some(Ok(())) => {
                    debug!("done");
                    let _ = tx.send_blocking("done".to_owned());
                }
                Some(Err(e)) => {
                    debug!("Error: {e}");
                    let _ = tx.send_blocking(format!("Error: {e}"));
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
    let config_btn = config_button.clone();
    let notify_data = data_for_notify.clone();
    let notify_app = app.clone();
    let track_view_done = track_view.clone();
    let status_clone = status.clone();

    glib::spawn_future_local(async move {
        while let Ok(msg) = rx.recv().await {
            status_clone.remove_all(context_id);
            status_clone.push(context_id, &msg);

            if msg == "done" || msg == "aborted" {
                scan_btn.set_sensitive(true);
                go_btn.set_sensitive(true);
                stop_btn.set_sensitive(false);
                config_btn.set_sensitive(true);
                track_view_done.set_sensitive(true);
                if msg == "done" {
                    notify_rip_complete(&notify_app, &notify_data);
                    let config = read_config();
                    if config.create_playlist
                        && let Ok(data) = notify_data.read()
                        && let Some(disc) = &data.disc
                        && let Err(e) = create_playlist(disc)
                    {
                        warn!("Failed to create playlist: {e}");
                    }
                    if config.eject_when_done {
                        eject_cd();
                    }
                }
                break;
            }
        }
    });
}

fn notify_rip_complete(app: &Application, data: &Arc<RwLock<Data>>) {
    if let Ok(state) = data.read()
        && let Some(disc) = &state.disc
    {
        debug!("Sending rip complete notification");
        let summary = "Ripping complete";
        let body = format!("{} - {}", disc.artist, disc.title);
        let notification = gio::Notification::new(summary);
        notification.set_body(Some(&body));
        app.send_notification(Some("rip-complete"), &notification);
    }
}
