use crate::{
    data::{Config, Disc, Encoder, Track},
    util::read_config,
};
use anyhow::{Result, anyhow};
use async_channel::Sender;
use glib::ControlFlow;
use gstreamer::{
    ClockTime, Element, ElementFactory, Format, GenericFormattedValue, MessageView, Pipeline,
    State, TagList, TagMergeMode, TagSetter, URIType,
    format::Percent,
    glib,
    glib::MainLoop,
    prelude::*,
    tags::{Album, Artist, Composer, Date, Duration, Title, TrackNumber},
};
use log::{debug, error};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};

/// Extract/Rip a `Disc` to the configured format
pub fn extract(disc: &Disc, status: &Sender<String>, ripping: &Arc<RwLock<bool>>) -> Result<()> {
    for track in &disc.tracks {
        if !is_ripping(ripping) {
            debug!("Ripping aborted by user");
            break;
        }
        if track.rip {
            let pipeline = create_pipeline(track, disc)?;
            extract_track(pipeline, &track.title, status, ripping.clone())?;
        }
    }
    Ok(())
}

/// Check if we should continue ripping
fn is_ripping(ripping: &Arc<RwLock<bool>>) -> bool {
    ripping.read().map(|r| *r).unwrap_or(false)
}

/// Rip one track using the provided pipeline
fn extract_track(
    pipeline: Pipeline,
    title: &str,
    status: &Sender<String>,
    ripping: Arc<RwLock<bool>>,
) -> Result<()> {
    let status_message = format!("Encoding {title}");
    let _ = status.send_blocking(status_message.clone());

    let main_loop = MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    pipeline.set_state(State::Playing)?;

    let working = Arc::new(RwLock::new(true));
    start_progress_updates(
        status_message,
        pipeline.clone(),
        ripping,
        status.clone(),
        working.clone(),
    );

    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("Pipeline has no bus"))?;
    let status_clone = status.clone();
    let last_error = Arc::new(RwLock::new(None));
    let last_error_clone = last_error.clone();

    let _guard = bus.add_watch(move |_, msg| {
        match msg.view() {
            MessageView::Eos(..) => {
                debug!("End of stream");
                set_working(&working, false);
                let _ = pipeline.set_state(State::Null);
                main_loop_clone.quit();
            }
            MessageView::Error(err) => {
                let _ = status_clone.send_blocking("aborted".to_owned());
                set_working(&working, false);
                if let Ok(mut e) = last_error_clone.write() {
                    *e = Some(format!(
                        "GStreamer error from {:?}: {} ({:?})",
                        err.src().map(GstObjectExt::path_string),
                        err.error(),
                        err.debug()
                    ));
                }
                error!(
                    "GStreamer error from {:?}: {} ({:?})",
                    err.src().map(GstObjectExt::path_string),
                    err.error(),
                    err.debug()
                );
                let _ = pipeline.set_state(State::Null);
                main_loop_clone.quit();
            }
            _ => (),
        }
        ControlFlow::Continue
    })?;

    main_loop.run();
    if let Ok(e) = last_error.read()
        && let Some(msg) = e.as_ref()
    {
        return Err(anyhow!(msg.clone()));
    }
    debug!("Finished encoding {title}");
    Ok(())
}

/// Set the working flag
fn set_working(working: &Arc<RwLock<bool>>, value: bool) {
    if let Ok(mut w) = working.write() {
        *w = value;
    }
}

/// Start periodic progress updates
fn start_progress_updates(
    status_message: String,
    pipeline: Pipeline,
    ripping: Arc<RwLock<bool>>,
    status: Sender<String>,
    working: Arc<RwLock<bool>>,
) {
    glib::timeout_add(std::time::Duration::from_millis(1000), move || {
        let should_continue = is_ripping(&ripping) && working.read().map(|w| *w).unwrap_or(false);

        if !should_continue {
            return ControlFlow::Break;
        }

        let percent = calculate_progress(&pipeline);
        let msg = format!("{status_message} : {percent:.0} %");
        let _ = status.send_blocking(msg);

        ControlFlow::Continue
    });
}

/// Calculate pipeline progress as percentage
#[allow(clippy::cast_precision_loss)]
fn calculate_progress(pipeline: &Pipeline) -> f64 {
    let zero = GenericFormattedValue::Percent(Some(Percent::from_percent(0)));
    let one = GenericFormattedValue::Percent(Some(Percent::from_percent(1)));

    let pos = pipeline
        .query_position_generic(Format::Percent)
        .unwrap_or(zero);
    let dur = pipeline
        .query_duration_generic(Format::Percent)
        .unwrap_or(one);

    if dur.value() == 0 {
        0.0
    } else {
        pos.value() as f64 / dur.value() as f64 * 100.0
    }
}

/// Create a `GStreamer` pipeline for encoding a track
fn create_pipeline(track: &Track, disc: &Disc) -> Result<Pipeline> {
    let config: Config = read_config();

    let extractor = create_cd_source(track.number)?;
    let tags = build_tags(track, disc)?;
    let output_path = build_output_path(&config, disc, track)?;
    let sink = create_file_sink(&output_path)?;

    let pipeline = Pipeline::new();

    match config.encoder {
        Encoder::MP3 => build_mp3_pipeline(&pipeline, extractor, sink, &tags, config.quality)?,
        Encoder::OGG => build_ogg_pipeline(&pipeline, extractor, sink, &tags, config.quality)?,
        Encoder::FLAC => build_flac_pipeline(&pipeline, extractor, sink, &tags, config.quality)?,
        Encoder::OPUS => build_opus_pipeline(&pipeline, extractor, sink, &tags, config.quality)?,
    }

    Ok(pipeline)
}

/// Create the CD audio source element
fn create_cd_source(track_number: u32) -> Result<Element> {
    let uri = format!("cdda://{track_number}");
    let extractor = Element::make_from_uri(URIType::Src, &uri, Some("cd_src"))?;
    extractor.set_property("read-speed", 0_i32);
    Ok(extractor)
}

/// Build the tag list for the track
fn build_tags(track: &Track, disc: &Disc) -> Result<TagList> {
    let mut tags = TagList::new();
    let tags_mut = tags
        .get_mut()
        .ok_or_else(|| anyhow!("Cannot get mutable tags"))?;

    tags_mut.add::<Title>(&track.title.as_str(), TagMergeMode::ReplaceAll);
    tags_mut.add::<Artist>(&track.artist.as_str(), TagMergeMode::ReplaceAll);
    tags_mut.add::<TrackNumber>(&track.number, TagMergeMode::ReplaceAll);
    tags_mut.add::<Album>(&disc.title.as_str(), TagMergeMode::ReplaceAll);
    tags_mut.add::<Duration>(
        &(ClockTime::SECOND * track.duration),
        TagMergeMode::ReplaceAll,
    );

    if let Some(year) = disc.year {
        let date = glib::Date::from_dmy(1, glib::DateMonth::January, year)?;
        tags_mut.add::<Date>(&date, TagMergeMode::ReplaceAll);
    }

    if let Some(ref composer) = track.composer {
        tags_mut.add::<Composer>(&composer.as_str(), TagMergeMode::ReplaceAll);
    }

    Ok(tags)
}

/// Build the output file path and ensure directory exists
fn build_output_path(config: &Config, disc: &Disc, track: &Track) -> Result<String> {
    let extension = config.encoder.file_extension();
    let artist = sanitize_path_component(&disc.artist);
    let album = sanitize_path_component(&disc.title);
    let title = sanitize_path_component(&track.title);
    let path = format!(
        "{}/{}-{}/{} - {}{}",
        config.encode_path, artist, album, track.number, title, extension
    );

    let parent = Path::new(&path)
        .parent()
        .ok_or_else(|| anyhow!("Invalid output path"))?;
    std::fs::create_dir_all(parent)?;

    Ok(path)
}

fn sanitize_path_component(value: &str) -> String {
    let mut out: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    out = out.trim().to_string();
    if out.is_empty() {
        "Unknown".to_string()
    } else {
        out
    }
}

/// Create file sink element
fn create_file_sink(location: &str) -> Result<Element> {
    let sink = ElementFactory::make("filesink").build()?;
    sink.set_property("location", location);
    Ok(sink)
}

/// Apply tags to an element that implements `TagSetter`
fn apply_tags(element: &Element, tags: &TagList) -> Result<()> {
    let tagsetter = element
        .dynamic_cast_ref::<TagSetter>()
        .ok_or_else(|| anyhow!("Element does not support TagSetter"))?;
    tagsetter.merge_tags(tags, TagMergeMode::ReplaceAll);
    Ok(())
}

/// Link elements and add to pipeline
fn link_pipeline(pipeline: &Pipeline, elements: &[&Element]) -> Result<()> {
    pipeline.add_many(elements)?;
    Element::link_many(elements)?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)] // Elements are consumed by pipeline
fn build_mp3_pipeline(
    pipeline: &Pipeline,
    source: Element,
    sink: Element,
    tags: &TagList,
    quality: crate::data::Quality,
) -> Result<()> {
    let encoder = ElementFactory::make("lamemp3enc").build()?;
    encoder.set_property("quality", quality.mp3_quality());

    let muxer = ElementFactory::make("id3v2mux").build()?;
    apply_tags(&muxer, tags)?;

    link_pipeline(pipeline, &[&source, &encoder, &muxer, &sink])
}

#[allow(clippy::needless_pass_by_value)] // Elements are consumed by pipeline
fn build_ogg_pipeline(
    pipeline: &Pipeline,
    source: Element,
    sink: Element,
    tags: &TagList,
    quality: crate::data::Quality,
) -> Result<()> {
    let convert = ElementFactory::make("audioconvert").build()?;
    let encoder = ElementFactory::make("vorbisenc").build()?;
    encoder.set_property("quality", quality.vorbis_quality());
    apply_tags(&encoder, tags)?;

    let muxer = ElementFactory::make("oggmux").build()?;

    link_pipeline(pipeline, &[&source, &convert, &encoder, &muxer, &sink])
}

#[allow(clippy::needless_pass_by_value)] // Elements are consumed by pipeline
fn build_flac_pipeline(
    pipeline: &Pipeline,
    source: Element,
    sink: Element,
    tags: &TagList,
    quality: crate::data::Quality,
) -> Result<()> {
    let encoder = ElementFactory::make("flacenc").build()?;
    encoder.set_property_from_str("quality", quality.flac_level());
    apply_tags(&encoder, tags)?;

    link_pipeline(pipeline, &[&source, &encoder, &sink])
}

#[allow(clippy::needless_pass_by_value)] // Elements are consumed by pipeline
fn build_opus_pipeline(
    pipeline: &Pipeline,
    source: Element,
    sink: Element,
    tags: &TagList,
    quality: crate::data::Quality,
) -> Result<()> {
    let convert = ElementFactory::make("audioconvert").build()?;
    let resample = ElementFactory::make("audioresample").build()?;
    let encoder = ElementFactory::make("opusenc").build()?;
    encoder.set_property("bitrate", quality.opus_bitrate());
    apply_tags(&encoder, tags)?;

    let muxer = ElementFactory::make("oggmux").build()?;

    link_pipeline(
        pipeline,
        &[&source, &convert, &resample, &encoder, &muxer, &sink],
    )
}

#[cfg(test)]
mod test {
    use anyhow::{Result, anyhow};
    use gstreamer::{Element, ElementFactory, Pipeline, prelude::*};
    use serial_test::serial;
    use std::{
        env,
        fs::{File, remove_file},
        io::Read,
        path::Path,
        sync::{Arc, RwLock},
    };

    use super::extract_track;

    fn test_wav_path() -> Result<String> {
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
        Ok(path)
    }

    /// Verify file type by checking magic bytes
    fn verify_file_type(path: &str, expected: &FileType) -> Result<()> {
        let mut file = File::open(path)?;
        let mut header = [0u8; 12];
        file.read_exact(&mut header)?;

        let detected = match &header {
            // MP3: ID3 tag or frame sync
            [0x49, 0x44, 0x33, ..] | [0xff, 0xfb | 0xfa, ..] => FileType::Mp3, // ID3v2 || Frame sync
            // FLAC: "fLaC"
            [0x66, 0x4c, 0x61, 0x43, ..] => FileType::Flac,
            // OGG: "OggS"
            [0x4f, 0x67, 0x67, 0x53, ..] => FileType::Ogg,
            _ => return Err(anyhow!("Unknown file type: {:02x?}", &header[..4])),
        };

        if &detected == expected {
            Ok(())
        } else {
            Err(anyhow!("Expected {expected:?}, got {detected:?}"))
        }
    }

    #[derive(Debug, PartialEq)]
    enum FileType {
        Mp3,
        Flac,
        Ogg, // Vorbis and Opus both use OGG container
    }

    #[test]
    #[serial]
    pub fn test_bad_pipeline() -> Result<()> {
        gstreamer::init()?;
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/blah.wav");

        let file = ElementFactory::make("filesrc").build()?;
        file.set_property("location", &path);
        let sink = ElementFactory::make("filesink").build()?;
        sink.set_property("location", "/tmp/file_example_WAV_1MG.mp3");

        let pipeline = Pipeline::new();
        let elements = &[&file, &sink];
        pipeline.add_many(elements)?;
        Element::link_many(elements)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        let result = extract_track(pipeline, "track", &tx, ripping);
        // Pipeline fails because filesrc->filesink is invalid (incompatible elements)
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_mp3() -> Result<()> {
        gstreamer::init()?;
        let path = test_wav_path()?;

        let file = ElementFactory::make("filesrc").build()?;
        file.set_property("location", &path);
        let wav = ElementFactory::make("wavparse").build()?;
        let encoder = ElementFactory::make("lamemp3enc").build()?;
        let id3 = ElementFactory::make("id3v2mux").build()?;
        let sink = ElementFactory::make("filesink").build()?;
        let dest = "/tmp/file_example_WAV_1MG.mp3";
        sink.set_property("location", dest);

        let pipeline = Pipeline::new();
        let elements = &[&file, &wav, &encoder, &id3, &sink];
        pipeline.add_many(elements)?;
        Element::link_many(elements)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping)?;

        assert!(Path::new(dest).exists());
        assert!(Path::new(dest).is_file());
        verify_file_type(dest, &FileType::Mp3)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_flac() -> Result<()> {
        gstreamer::init()?;
        let path = test_wav_path()?;

        let file = ElementFactory::make("filesrc").build()?;
        file.set_property("location", &path);
        let wav = ElementFactory::make("wavparse").build()?;
        let encoder = ElementFactory::make("flacenc").build()?;
        let sink = ElementFactory::make("filesink").build()?;
        let dest = "/tmp/file_example_WAV_1MG.flac";
        sink.set_property("location", dest);

        let pipeline = Pipeline::new();
        let elements = &[&file, &wav, &encoder, &sink];
        pipeline.add_many(elements)?;
        Element::link_many(elements)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping)?;

        assert!(Path::new(dest).exists());
        assert!(Path::new(dest).is_file());
        verify_file_type(dest, &FileType::Flac)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_opus() -> Result<()> {
        gstreamer::init()?;
        let path = test_wav_path()?;

        let file = ElementFactory::make("filesrc").build()?;
        file.set_property("location", &path);
        let wav = ElementFactory::make("wavparse").build()?;
        let convert = ElementFactory::make("audioconvert").build()?;
        let encoder = ElementFactory::make("opusenc").build()?;
        let mux = ElementFactory::make("oggmux").build()?;
        let sink = ElementFactory::make("filesink").build()?;
        let dest = "/tmp/file_example_WAV_1MG-opus.ogg";
        sink.set_property("location", dest);

        let pipeline = Pipeline::new();
        let elements = &[&file, &wav, &convert, &encoder, &mux, &sink];
        pipeline.add_many(elements)?;
        Element::link_many(elements)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping)?;

        assert!(Path::new(dest).exists());
        assert!(Path::new(dest).is_file());
        verify_file_type(dest, &FileType::Ogg)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_ogg() -> Result<()> {
        gstreamer::init()?;
        let path = test_wav_path()?;

        let file = ElementFactory::make("filesrc").build()?;
        file.set_property("location", &path);
        let wav = ElementFactory::make("wavparse").build()?;
        let convert = ElementFactory::make("audioconvert").build()?;
        let vorbis = ElementFactory::make("vorbisenc").build()?;
        let mux = ElementFactory::make("oggmux").build()?;
        let sink = ElementFactory::make("filesink").build()?;
        let dest = "/tmp/file_example_WAV_1MG.ogg";
        sink.set_property("location", dest);

        let pipeline = Pipeline::new();
        let elements = &[&file, &wav, &convert, &vorbis, &mux, &sink];
        pipeline.add_many(elements)?;
        Element::link_many(elements)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        extract_track(pipeline, "track", &tx, ripping)?;

        assert!(Path::new(dest).exists());
        assert!(Path::new(dest).is_file());
        verify_file_type(dest, &FileType::Ogg)?;
        remove_file(dest)?;
        Ok(())
    }
}
