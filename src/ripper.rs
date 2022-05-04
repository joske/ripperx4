use std::path::Path;

use gstreamer::tags::{Album, Artist, Duration, TrackNumber, Composer};
use gstreamer::*;
use gstreamer::{prelude::*, tags::Title};

use crate::data::{Disc, Track};
pub fn extract(disc: &Disc, status: &glib::Sender<String>) {
    for t in disc.tracks.iter() {
        extract_track(&disc, &t, status);
    }
}

fn extract_track(disc: &Disc, track: &Track, status: &glib::Sender<String>) {
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
    let location = format!("{}/Music/{}-{}/{}.mp3", home.display(), disc.artist, disc.title, track.title);
    //ensure folder exists
    std::fs::create_dir_all(Path::new(&location).parent().unwrap()).unwrap();
    let sink = ElementFactory::make("filesink", None).unwrap();
    sink.set_property("location", location);
    let pipeline = Pipeline::new(Some("ripper"));
    let elements = &[&extractor, &progress, &encoder, &id3, &sink];
    pipeline.add_many(elements).unwrap();
    Element::link_many(elements).unwrap();
    let status_message = format!("encoding {}", track.title);
    status.send(status_message).unwrap();

    pipeline.set_state(State::Playing).unwrap();

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            MessageView::Progress(p) => {
                println!("progress: ${:?}", p);
                break;
            }
            MessageView::StepStart(p) => {
                println!("step start: ${:?}", p);
                break;
            }
            MessageView::StepDone(p) => {
                println!("step done: ${:?}", p);
                break;
            }
            MessageView::SegmentStart(p) => {
                println!("segment start: ${:?}", p);
                break;
            }
            MessageView::SegmentDone(p) => {
                println!("segment done: ${:?}", p);
                break;
            }
            MessageView::Eos(..) => {
                pipeline.set_state(State::Null).unwrap();
                println!("done");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(State::Null).unwrap();
                println!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            MessageView::StateChanged(s) => {
                println!(
                    "State changed from {:?}: {:?} -> {:?} ({:?})",
                    s.src().map(|s| s.path_string()),
                    s.old(),
                    s.current(),
                    s.pending()
                );
            }
            _ => (),
        }
    }

    pipeline.set_state(State::Null).unwrap();
}
