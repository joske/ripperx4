use std::{
    error::Error,
    fmt::Display,
    path::Path,
    sync::{Arc, RwLock},
};

use crate::data::{Config, Disc, Encoder, Track};
use gstreamer::{
    format::Percent,
    glib,
    glib::MainLoop,
    prelude::*,
    tags::{Album, Artist, Composer, Date, Duration, Title, TrackNumber},
    ClockTime, Element, ElementFactory, Format, GenericFormattedValue, MessageView, Pipeline,
    State, TagList, TagMergeMode, TagSetter, URIType,
};

#[derive(Debug)]
struct MyError(String);

impl Error for MyError {}

impl Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "aargh: {}", self.0)
    }
}

/// Extract/Rip a `Disc` to MP3/OGG/FLAC
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

/// Rip one `Track`
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
                .unwrap_or(GenericFormattedValue::Percent(Some(Percent::from_percent(
                    0,
                ))))
                .value() as f64
                / dur
                    .unwrap_or(GenericFormattedValue::Percent(Some(Percent::from_percent(
                        1,
                    ))))
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

/// Create a gstreamer pipeline for extracting/encoding the `Track`
/// Returns a linked `Pipeline`
fn create_pipeline(track: &Track, disc: &Disc) -> Result<Pipeline, Box<dyn Error>> {
    let config: Config = confy::load("ripperx4", None)?;

    gstreamer::init()?;

    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, cdda.as_str(), Some("cd_src"))?;
    extractor.set_property("read-speed", 0_i32);

    let id3 = ElementFactory::make("id3v2mux").build()?;
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
    let sink = ElementFactory::make("filesink").build()?;
    sink.set_property("location", location);

    let pipeline = Pipeline::new(Some("ripper"));
    match config.encoder {
        Encoder::MP3 => {
            let enc = ElementFactory::make("lamemp3enc").build()?;
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
            let convert = ElementFactory::make("audioconvert").build()?;
            let vorbis = ElementFactory::make("vorbisenc").build()?;
            let mux = ElementFactory::make("oggmux").build()?;

            let tagsetter = &vorbis
                .dynamic_cast_ref::<TagSetter>()
                .ok_or("failed to cast")?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &convert, &vorbis, &mux, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::FLAC => {
            let enc = ElementFactory::make("flacenc").build()?;
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
    use gstreamer::{prelude::*, *};
    use serial_test::serial;
    use std::{
        env,
        sync::{Arc, RwLock},
    };

    use super::extract_track;

    #[test]
    #[serial]
    pub fn test_mp3() {
        gstreamer::init().unwrap();
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/file_example_WAV_1MG.wav");

        let file = ElementFactory::make("filesrc").build().unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse").build().unwrap();
        let encoder = ElementFactory::make("lamemp3enc").build().unwrap();
        let id3 = ElementFactory::make("id3v2mux").build().unwrap();
        let sink = ElementFactory::make("filesink").build().unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.mp3");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &id3, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| {
            let s = value;
            {
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
        let file = ElementFactory::make("filesrc").build().unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse").build().unwrap();
        let encoder = ElementFactory::make("flacenc").build().unwrap();
        let sink = ElementFactory::make("filesink").build().unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.flac");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| {
            let s = value;
            {
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
        let file = ElementFactory::make("filesrc").build().unwrap();
        file.set_property("location", path.as_str());
        let wav = ElementFactory::make("wavparse").build().unwrap();
        let convert = ElementFactory::make("audioconvert").build().unwrap();
        let vorbis = ElementFactory::make("vorbisenc").build().unwrap();
        let mux = ElementFactory::make("oggmux").build().unwrap();
        let sink = ElementFactory::make("filesink").build().unwrap();
        sink.set_property("location", "/tmp/file_example_WAV_1MG.ogg");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &convert, &vorbis, &mux, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| {
            let s = value;
            {
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping).ok();
    }
}
