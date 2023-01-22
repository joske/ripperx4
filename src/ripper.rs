use std::error::Error;
use std::fmt::Display;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::data::{Config, Disc, Encoder, Track};
use gstreamer::format::Percent;
use gstreamer::glib::MainLoop;
use gstreamer::tags::{Album, Artist, Composer, Date, Duration, TrackNumber};
use gstreamer::{
    glib, ClockTime, Element, ElementFactory, Format, GenericFormattedValue, MessageView, Pipeline,
    State, TagList, TagMergeMode, TagSetter, URIType,
};
use gstreamer::{prelude::*, tags::Title};

#[derive(Debug)]
struct MyError(String);

impl Error for MyError {}

impl Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "aargh: {}", self.0)
    }
}

pub fn extract(
    disc: &Disc,
    status: &glib::Sender<String>,
    ripping: &Arc<RwLock<bool>>,
) -> Result<(), Box<dyn Error>> {
    for t in &disc.tracks {
        if !*ripping.read().unwrap() {
            // ABORTED
            break;
        }
        let pipeline = create_pipeline(t, disc)?;
        extract_track(pipeline, t.title.as_str(), status, ripping.clone())?;
    }
    Ok(())
}

fn extract_track(
    pipeline: Pipeline,
    title: &str,
    status: &glib::Sender<String>,
    ripping: Arc<RwLock<bool>>,
) -> Result<(), Box<dyn Error>> {
    let status_message = format!("encoding {}", title);
    let status_message_clone = status_message.clone();
    status.send(status_message)?;

    let playing = Arc::new(RwLock::new(false));
    let main_loop = MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    pipeline.set_state(State::Playing)?;
    let pipeline_clone = pipeline.clone();
    let status = status.clone();
    glib::timeout_add(std::time::Duration::from_millis(1000), move || {
        let pipeline = &pipeline_clone;
        if !*ripping.read().unwrap() {
            // ABORTED
            pipeline.set_state(State::Null).ok();
            status.send("aborted".to_owned()).ok();
            return glib::Continue(false);
        }
        let pos = pipeline.query_position_generic(Format::Percent);
        let dur = pipeline.query_duration_generic(Format::Percent);
        if pos.is_some() && dur.is_some() {
            let perc = pos
                .unwrap_or(GenericFormattedValue::Percent(Some(Percent(0))))
                .value() as f64
                / dur
                    .unwrap_or(GenericFormattedValue::Percent(Some(Percent(1))))
                    .value() as f64
                * 100.0;
            let status_message_perc = format!("{} : {:.0} %", status_message_clone, perc);
            status.send(status_message_perc).ok();

            if pos == dur {}
        } else {
            return glib::Continue(false);
        }
        glib::Continue(true)
    });

    let bus = pipeline.bus().ok_or_else(|| MyError("no bus".to_owned()))?;

    bus.add_watch(move |_, msg| {
        let main_loop = &main_loop_clone;
        match msg.view() {
            MessageView::Eos(..) => {
                pipeline.set_state(State::Null).ok();
                main_loop.quit();
            }
            MessageView::Error(err) => {
                pipeline.set_state(State::Null).ok();
                println!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                main_loop.quit();
            }
            MessageView::StateChanged(s) => {
                println!(
                    "State changed from {:?}: {:?} -> {:?} ({:?})",
                    s.src().map(|s| s.path_string()),
                    s.old(),
                    s.current(),
                    s.pending()
                );
                if s.current() == State::Playing {
                    *playing.clone().write().unwrap() = true;
                }
            }
            _ => (),
        }
        glib::Continue(true)
    })?;
    main_loop.run();
    Ok(())
}

fn create_pipeline(track: &Track, disc: &Disc) -> Result<Pipeline, Box<dyn Error>> {
    let config: Config = confy::load("ripperx4", None)?;

    gstreamer::init()?;

    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, cdda.as_str(), Some("cd_src"))?;
    extractor.set_property("read-speed", 0_i32);

    let id3 = ElementFactory::make("id3v2mux", None)?;
    let mut tags = TagList::new();
    {
        let tags = tags
            .get_mut()
            .ok_or_else(|| MyError("can not get mut".to_owned()))?;
        tags.add::<Title>(&track.title.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<Artist>(&track.artist.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<TrackNumber>(&track.number, TagMergeMode::ReplaceAll);
        tags.add::<Album>(&disc.title.as_str(), TagMergeMode::ReplaceAll);
        if let Some(year) = disc.year {
            let date = glib::Date::from_dmy(1, glib::DateMonth::January, year)?;
            tags.add::<Date>(&date, TagMergeMode::ReplaceAll);
        }
        tags.add::<Duration>(
            &(ClockTime::SECOND * track.duration),
            TagMergeMode::ReplaceAll,
        );
        if let Some(composer) = track.composer.clone() {
            tags.add::<Composer>(&composer.as_str(), TagMergeMode::ReplaceAll);
        }
    }

    let extension = match config.encoder {
        Encoder::MP3 => ".mp3",
        Encoder::OGG => ".ogg",
        Encoder::FLAC => ".flac",
    };

    let location = format!(
        "{}/{}-{}/{}{}",
        config.encode_path, disc.artist, disc.title, track.title, extension
    );
    //ensure folder exists
    std::fs::create_dir_all(
        Path::new(&location)
            .parent()
            .ok_or_else(|| MyError("failed to create folder".to_owned()))?,
    )?;
    let sink = ElementFactory::make("filesink", None)?;
    sink.set_property("location", location);

    let pipeline = Pipeline::new(Some("ripper"));
    match config.encoder {
        Encoder::MP3 => {
            let enc = ElementFactory::make("lamemp3enc", None)?;
            enc.set_property("bitrate", 320_i32);
            enc.set_property("quality", 0_f32);

            let tagsetter = &id3
                .dynamic_cast_ref::<TagSetter>()
                .ok_or("failed to cast")?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &enc, &id3, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::OGG => {
            let convert = ElementFactory::make("audioconvert", None)?;
            let vorbis = ElementFactory::make("vorbisenc", None)?;
            let mux = ElementFactory::make("oggmux", None)?;

            let tagsetter = &vorbis
                .dynamic_cast_ref::<TagSetter>()
                .ok_or("failed to cast")?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &convert, &vorbis, &mux, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::FLAC => {
            let enc = ElementFactory::make("flacenc", None)?;
            let elements = &[&extractor, &enc, &id3, &sink];

            let tagsetter = &id3
                .dynamic_cast_ref::<TagSetter>()
                .ok_or("failed to cast")?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
    };

    Ok(pipeline)
}

#[cfg(test)]
mod test {
    use gstreamer::prelude::*;
    use gstreamer::*;
    use serial_test::serial;
    use std::env;
    use std::sync::{Arc, RwLock};

    use super::extract_track;

    #[test]
    #[serial]
    pub fn test_mp3() {
        gstreamer::init().unwrap();
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/file_example_WAV_1MG.wav");

        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let encoder = ElementFactory::make("lamemp3enc", None).unwrap();
        let id3 = ElementFactory::make("id3v2mux", None).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.mp3");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &id3, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping).ok();
    }

    #[test]
    #[serial]
    pub fn test_flac() {
        gstreamer::init().unwrap();
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let encoder = ElementFactory::make("flacenc", None).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.flac");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping).ok();
    }

    #[test]
    #[serial]
    pub fn test_ogg() {
        gstreamer::init().unwrap();
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let convert = ElementFactory::make("audioconvert", None).unwrap();
        let vorbis = ElementFactory::make("vorbisenc", None).unwrap();
        let mux = ElementFactory::make("oggmux", None).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.ogg");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &convert, &vorbis, &mux, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping).ok();
    }

    #[test]
    #[serial]
    pub fn test_parse() {
        gstreamer::init().unwrap();
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
        let desc = format!(
            r##"filesrc location={} ! wavparse ! audioconvert ! vorbisenc ! oggmux ! filesink location=out.ogg"##,
            path
        );
        // Like teasered above, we use GLib's main loop to operate GStreamer's bus.
        let main_loop = glib::MainLoop::new(None, false);

        let pipeline = gstreamer::parse_launch(desc.as_str()).unwrap();
        let bus = pipeline.bus().unwrap();

        pipeline
            .set_state(gstreamer::State::Playing)
            .expect("Unable to set the pipeline to the `Playing` state");

        let main_loop_clone = main_loop.clone();

        //bus.add_signal_watch();
        //bus.connect_message(None, move |_, msg| {
        bus.add_watch(move |_, msg| {
            use gstreamer::MessageView;

            let main_loop = &main_loop_clone;
            match msg.view() {
                MessageView::Eos(..) => main_loop.quit(),
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    main_loop.quit();
                }
                _ => (),
            };

            glib::Continue(true)
        })
        .expect("Failed to add bus watch");

        main_loop.run();

        pipeline
            .set_state(gstreamer::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");

        // Here we remove the bus watch we added above. This avoids a memory leak, that might
        // otherwise happen because we moved a strong reference (clone of main_loop) into the
        // callback closure above.
        bus.remove_watch().unwrap();
    }
}
