use std::path::Path;
use std::sync::{Arc, RwLock};

use glib::MainLoop;
use gstreamer::tags::{Album, Artist, Composer, Duration, TrackNumber};
use gstreamer::*;
use gstreamer::{prelude::*, tags::Title};

use crate::data::{Disc, Track};

pub fn extract(disc: &Disc, status: &glib::Sender<String>, ripping: Arc<RwLock<bool>>) {
    for t in disc.tracks.iter() {
        if !*ripping.read().unwrap() {
            // ABORTED
            break;
        }
        let pipeline = create_pipeline(t, disc);
        extract_track(pipeline, t.title.clone(), status);
    }
}

fn extract_track(pipeline: Pipeline, title: String, status: &glib::Sender<String>) {
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
    glib::timeout_add(std::time::Duration::from_millis(10), move || {
        if *playing_clone.read().unwrap() {
            let pipeline = &pipeline_clone;
            let pos = pipeline.query_position_generic(Format::Time).unwrap();
            let dur = pipeline.query_duration_generic(Format::Time).unwrap();
            println!("position: {} / {}", pos, dur);
            let status_message_perc = format!("encoding {}", status_message_clone);
            status.send(status_message_perc).unwrap();

            if pos == dur {
                return glib::Continue(false);
            }
        } else {
            println!("not yet playing");
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
                println!("done");
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
    gstreamer::init().unwrap();
    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, cdda.as_str(), Some("cd_src")).unwrap();
    extractor.set_property("read-speed", 0 as i32);
    let progress = ElementFactory::make("progressreport", None).unwrap();
    progress.set_property("update-freq", 1 as i32);
    let encoder = ElementFactory::make("lamemp3enc", None).unwrap();
    encoder.set_property("bitrate", 320 as i32);
    encoder.set_property("quality", 0 as f32);
    let id3 = ElementFactory::make("id3v2mux", None).unwrap();
    let mut tags = TagList::new();
    {
        let tags = tags.get_mut().unwrap();
        tags.add::<Title>(&track.title.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<Artist>(&track.artist.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<TrackNumber>(&track.number, TagMergeMode::ReplaceAll);
        tags.add::<Album>(&disc.title.as_str(), TagMergeMode::ReplaceAll);
        tags.add::<Duration>(
            &(ClockTime::SECOND * track.duration),
            TagMergeMode::ReplaceAll,
        );
        if track.composer.is_some() {
            let c = track.composer.clone().unwrap();
            tags.add::<Composer>(&c.as_str(), TagMergeMode::ReplaceAll);
        }
    }
    let tagsetter = &id3.dynamic_cast_ref::<TagSetter>().unwrap();
    tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);
    let home = home::home_dir().unwrap();
    let location = format!(
        "{}/Music/{}-{}/{}.mp3",
        home.display(),
        disc.artist,
        disc.title,
        track.title
    );
    //ensure folder exists
    std::fs::create_dir_all(Path::new(&location).parent().unwrap()).unwrap();
    let sink = ElementFactory::make("filesink", None).unwrap();
    sink.set_property("location", location);
    let pipeline = Pipeline::new(Some("ripper"));
    let elements = &[&extractor, &progress, &encoder, &id3, &sink];
    pipeline.add_many(elements).unwrap();
    Element::link_many(elements).unwrap();
    pipeline
}

#[cfg(test)]
mod test {
    use glib::MainLoop;
    use gstreamer::prelude::*;
    use gstreamer::*;

    use super::extract_track;

    #[test]
    pub fn test() {
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
        let main_loop = MainLoop::new(None, false);
        extract_track(pipeline, "track".to_owned(), &tx);
        rx.attach(None, move |value| match value {
            s => {
                println!("status: {}", s);
                if s == "done" {
                    return glib::Continue(false);
                }
                glib::Continue(true)
            }
        });
        main_loop.run();
    }
}
