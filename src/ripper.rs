use std::path::Path;
use std::sync::{Arc, RwLock};

use confy::ConfyError;
use glib::MainLoop;
use gstreamer::tags::{Album, Artist, Composer, Duration, TrackNumber, Date};
use gstreamer::*;
use gstreamer::{prelude::*, tags::Title};
use gnudb::{Disc, Track};
use crate::data::{Config, Encoder};

pub fn extract(disc: &Disc, status: &glib::Sender<String>, ripping: Arc<RwLock<bool>>) {
    for t in disc.tracks.iter() {
        if !*ripping.read().unwrap() {
            // ABORTED
            break;
        }
        let pipeline = create_pipeline(t, disc);
        extract_track(pipeline, t.title.clone(), status, ripping.clone());
    }
}

fn extract_track(pipeline: Pipeline, title: String, status: &glib::Sender<String>, ripping: Arc<RwLock<bool>>) {
    let status_message = format!("encoding {}", title);
    let status_message_clone = status_message.clone();
    status.send(status_message).unwrap();

    let playing = Arc::new(RwLock::new(false));
    let main_loop = MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    pipeline.set_state(State::Playing).unwrap();
    let pipeline_clone = pipeline.clone();
    let playing_clone = playing.clone();
    let status = status.clone();
    glib::timeout_add(std::time::Duration::from_millis(1000), move || {
        let pipeline = &pipeline_clone;
        if !*ripping.read().unwrap() {
            // ABORTED
            pipeline.set_state(State::Null).unwrap();
            status.send("aborted".to_owned()).unwrap();
            return glib::Continue(false);
        }
        if *playing_clone.read().unwrap() {
            let pos = pipeline.query_position_generic(Format::Percent);
            let dur = pipeline.query_duration_generic(Format::Percent);
            if pos.is_some() && dur.is_some() {
                let perc = pos.unwrap().value() as f64 / dur.unwrap().value() as f64 * 100.0;
                let status_message_perc = format!("{} : {:.0} %", status_message_clone, perc);
                status.send(status_message_perc).unwrap();
    
                if pos == dur {
                }
            } else {
                return glib::Continue(false);
            }
        }
        return glib::Continue(true);
    });

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    bus.add_watch(move |_, msg| {
        let main_loop = &main_loop_clone;
        match msg.view() {
            MessageView::Eos(..) => {
                pipeline.set_state(State::Null).unwrap();
                main_loop.quit();
            }
            MessageView::Error(err) => {
                pipeline.set_state(State::Null).unwrap();
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
    })
    .unwrap();
    main_loop.run();
}

fn create_pipeline(track: &Track, disc: &Disc) -> Pipeline {
    let cfg: Result<Config, ConfyError>  = confy::load("ripperx4");
    let config = cfg.unwrap();

    gstreamer::init().unwrap();
    
    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, cdda.as_str(), Some("cd_src")).unwrap();
    extractor.set_property("read-speed", 0 as i32);

    let id3 = ElementFactory::make("id3v2mux", None).unwrap();
    let mut tags = TagList::new();
    {
        let tags = tags.get_mut().unwrap();
        tags.add::<Title>(&track.title.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<Artist>(&track.artist.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<TrackNumber>(&track.number, TagMergeMode::ReplaceAll);
        tags.add::<Album>(&disc.title.as_str(), TagMergeMode::ReplaceAll);
        if disc.year.is_some() {
            let date = glib::Date::from_dmy(1, glib::DateMonth::January, disc.year.unwrap()).unwrap();
            tags.add::<Date>(&date, TagMergeMode::ReplaceAll);
        }
        tags.add::<Duration>(
            &(ClockTime::SECOND * track.duration),
            TagMergeMode::ReplaceAll,
        );
        if track.composer.is_some() {
            let c = track.composer.clone().unwrap();
            tags.add::<Composer>(&c.as_str(), TagMergeMode::ReplaceAll);
        }
    }

    let extension = match config.encoder {
        Encoder::MP3 =>".mp3",
        Encoder::OGG => ".ogg",
        Encoder::FLAC => ".flac",
    };

    let location = format!(
        "{}/{}-{}/{}{}",
        config.encode_path,
        disc.artist,
        disc.title,
        track.title,
        extension
    );
    //ensure folder exists
    std::fs::create_dir_all(Path::new(&location).parent().unwrap()).unwrap();
    let sink = ElementFactory::make("filesink", None).unwrap();
    sink.set_property("location", location);

    let pipeline = Pipeline::new(Some("ripper"));
    match config.encoder {
        Encoder::MP3 => {
            let enc = ElementFactory::make("lamemp3enc", None).unwrap();
            enc.set_property("bitrate", 320 as i32);
            enc.set_property("quality", 0 as f32);

            let tagsetter = &id3.dynamic_cast_ref::<TagSetter>().unwrap();
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &enc, &id3, &sink];
            pipeline.add_many(elements).unwrap();
            Element::link_many(elements).unwrap();
        },
        Encoder::OGG => {
            let convert = ElementFactory::make("audioconvert", None).unwrap();
            let vorbis = ElementFactory::make("vorbisenc", None).unwrap();
            let mux = ElementFactory::make("oggmux", None).unwrap();

            let tagsetter = &vorbis.dynamic_cast_ref::<TagSetter>().unwrap();
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &convert, &vorbis, &mux, &sink];
            pipeline.add_many(elements).unwrap();
            Element::link_many(elements).unwrap();
        }
        Encoder::FLAC => {
            let enc = ElementFactory::make("flacenc", None).unwrap();
            let elements = &[&extractor, &enc, &id3, &sink];

            let tagsetter = &id3.dynamic_cast_ref::<TagSetter>().unwrap();
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            pipeline.add_many(elements).unwrap();
            Element::link_many(elements).unwrap();
        }
    };

    pipeline
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock};

    use gstreamer::prelude::*;
    use gstreamer::*;

    use super::extract_track;

    #[test]
    pub fn test_mp3() {
        gstreamer::init().unwrap();
        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.wav");
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let encoder = ElementFactory::make("lamemp3enc", None).unwrap();
        let id3 = ElementFactory::make("id3v2mux", None).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.mp3");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &id3, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                println!("status: {}", s);
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track".to_owned(), &tx, ripping);
    }
    #[test]
    pub fn test_flac() {
        gstreamer::init().unwrap();
        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.wav");
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let encoder = ElementFactory::make("flacenc", None).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.flac");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &encoder, &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                println!("status: {}", s);
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track".to_owned(), &tx, ripping);
    }

    #[test]
    pub fn test_ogg() {
        gstreamer::init().unwrap();
        let file = ElementFactory::make("filesrc", None).unwrap();
        file.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.wav");
        let wav = ElementFactory::make("wavparse", None).unwrap();
        let bin = parse_bin_from_description("audioconvert ! vorbisenc ! oggmux", false).unwrap();
        let sink = ElementFactory::make("filesink", None).unwrap();
        sink.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.ogg");
        let pipeline = Pipeline::new(Some("ripper"));
        let elements = &[&file, &wav, &bin.dynamic_cast_ref::<Element>().unwrap(), &sink];
        pipeline.add_many(elements).unwrap();
        Element::link_many(elements).unwrap();
        let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        rx.attach(None, move |value| match value {
            s => {
                println!("status: {}", s);
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track".to_owned(), &tx, ripping);
    }

}
