use gstreamer::prelude::*;
use gstreamer::*;

pub fn extract() {
    gstreamer::init().unwrap();
    // let extractor = Element::make_from_uri(URIType::Src, "cdda://1", Some("cd_src")).unwrap();
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

    pipeline.set_state(State::Playing).unwrap();

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => {
                pipeline.set_state(State::Null).unwrap();
                println!("done");
                break;
            },
            MessageView::Error(err) => {
                pipeline.set_state(State::Null).unwrap();
                println!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            },
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
