use gstreamer::tags::{Album, Artist, Duration, TrackNumber, Composer};
use gstreamer::*;
use gstreamer::{prelude::*, tags::Title};

use crate::data::{Disc, Track};

pub fn extract(disc: &Disc, track: &Track) {
    gstreamer::init().unwrap();
    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, cdda.as_str(), Some("cd_src")).unwrap();
    extractor.set_property("read-speed", 40);
    // let file = ElementFactory::make("filesrc", None).unwrap();
    // file.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.wav");
    // let wav = ElementFactory::make("wavparse", None).unwrap();
    // let encodebin = gstreamer::parse_bin_from_description("lamemp3enc ! xingmux ! id3v2mux", true).unwrap();
    let encoder = ElementFactory::make("lamemp3enc", None).unwrap();
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

    let sink = ElementFactory::make("filesink", None).unwrap();
    sink.set_property("location", format!("/home/jos/Music/{}.mp3", track.title));
    let pipeline = Pipeline::new(Some("ripper"));
    let elements = &[&extractor, &encoder, &id3, &sink];
    pipeline.add_many(elements).unwrap();
    Element::link_many(elements).unwrap();

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
