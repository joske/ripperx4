use gstreamer::*;
use gstreamer::prelude::*;

pub fn extract() {
    let extractor = Element::make_from_uri(URIType::Src, "cdda://1", Some("cd_src")).unwrap();
    let encoderbin = parse_bin_from_description("lamemp3enc ! xingmux ! id3v2mux", true).unwrap();
    let pipeline = Pipeline::new(Some("ripper/encoder"));
    pipeline.add(&extractor).unwrap();
    pipeline.add(&encoderbin).unwrap();
    // let bus = pipeline.bus();
    pipeline.set_state(State::Playing).unwrap();
}
