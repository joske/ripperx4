use gstreamer::prelude::*;
use gstreamer::*;

pub fn extract() {
    gstreamer::init().unwrap();
    let location = "/home/jos/Downloads/file_example_WAV_1MG.wav";
    // let queue = ElementFactory::make("queue", None).unwrap();
    // let extractor = Element::make_from_uri(URIType::Src, "cdda://1", Some("cd_src")).unwrap();
    let file = ElementFactory::make("filesrc", None).unwrap();
    file.set_property("location", location);
    let wav = ElementFactory::make("wavparse", None).unwrap();
    let encoder = ElementFactory::make("lamemp3enc", None).unwrap();
    // let encoder = parse_bin_from_description("lamemp3enc ! id3v2mux", false).unwrap();
    let sink = ElementFactory::make("filesink", None).unwrap();
    sink.set_property("location", "/home/jos/Downloads/file_example_WAV_1MG.mp3");
    let pipeline = Pipeline::new(Some("ripper"));
    pipeline.add(&file).unwrap();
    pipeline.add(&wav).unwrap();
    pipeline.add(&encoder).unwrap();
    pipeline.add(&sink).unwrap();
    Element::link_many(&[&file, &wav, &encoder, &sink]).unwrap();

    // let pipeline_weak = pipeline.downgrade();
    // wav.connect_pad_added(move |dbin, src_pad| {
    //     let pipeline = match pipeline_weak.upgrade() {
    //         Some(pipeline) => pipeline,
    //         None => return,
    //     };
    //     println!("padd added");
    // });

    pipeline.set_state(State::Playing).unwrap();

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => break,
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
