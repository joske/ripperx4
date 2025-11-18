use crate::data::{Config, Disc, Encoder, Track};
use anyhow::{anyhow, Result};
use async_channel::Sender;
use glib::ControlFlow;
use gstreamer::{
    format::Percent,
    glib,
    glib::MainLoop,
    prelude::*,
    tags::{Album, Artist, Composer, Date, Duration, Title, TrackNumber},
    ClockTime, Element, ElementFactory, Format, GenericFormattedValue, MessageView, Pipeline,
    State, TagList, TagMergeMode, TagSetter, URIType,
};
use log::{debug, error};
use std::{
    path::Path,
    sync::{Arc, RwLock},
};

/// Extract/Rip a `Disc` to MP3/OGG/FLAC
pub fn extract(disc: &Disc, status: &Sender<String>, ripping: &Arc<RwLock<bool>>) -> Result<()> {
    for t in &disc.tracks {
        if !*ripping.read().expect("failed to get state") {
            // ABORTED
            break;
        }
        let pipeline = create_pipeline(t, disc)?;
        if t.rip {
            extract_track(pipeline, &t.title, status, ripping.clone())?;
        }
    }
    Ok(())
}

/// Rip one `Track`
fn extract_track(
    pipeline: Pipeline,
    title: &str,
    status: &Sender<String>,
    ripping: Arc<RwLock<bool>>,
) -> Result<()> {
    let status_message = format!("Encoding {title}");
    status.send_blocking(status_message.clone()).ok();

    let main_loop = MainLoop::new(None, false);
    let main_loop_clone = main_loop.clone();

    pipeline.set_state(State::Playing)?;
    let status = status.clone();
    let working = Arc::new(RwLock::new(true));
    handle_progress(
        status_message,
        pipeline.clone(),
        ripping,
        status.clone(),
        working.clone(),
    );

    let bus = pipeline.bus().ok_or(anyhow!("no bus".to_owned()))?;

    let guard = bus.add_watch(move |_, msg| {
        let main_loop = &main_loop_clone;
        match msg.view() {
            MessageView::Eos(..) => {
                debug!("Eos");
                let mut w = working.write().expect("failed to get state");
                *w = false;
                pipeline.set_state(State::Null).ok();
                main_loop.quit();
            }
            MessageView::Error(err) => {
                debug!("Error");
                status.send_blocking("aborted".to_owned()).ok();
                let mut w = working.write().expect("failed to get state");
                *w = false;
                error!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(gstreamer::prelude::GstObjectExt::path_string),
                    err.error(),
                    err.debug()
                );
                pipeline.set_state(State::Null).ok();
                main_loop.quit();
            }
            _ => (),
        }
        ControlFlow::Continue
    })?;
    main_loop.run();
    drop(guard);
    debug!("done with {title}");
    Ok(())
}

fn handle_progress(
    status_message: String,
    pipeline_clone: Pipeline,
    ripping: Arc<RwLock<bool>>,
    status: Sender<String>,
    working: Arc<RwLock<bool>>,
) {
    glib::timeout_add(std::time::Duration::from_millis(1000), move || {
        let pipeline = &pipeline_clone;
        if !*ripping.read().expect("failed to get state")
            || !*working.read().expect("failed to get state")
        {
            return ControlFlow::Break;
        }
        let zero = GenericFormattedValue::Percent(Some(Percent::from_percent(0)));
        let one = GenericFormattedValue::Percent(Some(Percent::from_percent(1)));
        let pos = pipeline
            .query_position_generic(Format::Percent)
            .unwrap_or(zero);
        let dur = pipeline
            .query_duration_generic(Format::Percent)
            .unwrap_or(one);
        let perc = pos.value() as f64 / dur.value() as f64 * 100.0;
        let status_message_perc = format!("{status_message} : {perc:.0} %");
        status.send_blocking(status_message_perc.clone()).ok();

        ControlFlow::Continue
    });
}

/// Create a gstreamer pipeline for extracting/encoding the `Track`
/// Returns a linked `Pipeline`
fn create_pipeline(track: &Track, disc: &Disc) -> Result<Pipeline> {
    let config: Config = confy::load("ripperx4", None)?;

    gstreamer::init()?;

    let cdda = format!("cdda://{}", track.number);
    let extractor = Element::make_from_uri(URIType::Src, &cdda, Some("cd_src"))?;
    extractor.set_property("read-speed", 0_i32);

    let id3 = ElementFactory::make("id3v2mux").build()?;
    let mut tags = TagList::new();
    {
        let tags = tags
            .get_mut()
            .ok_or(anyhow!("can not get mut".to_owned()))?;
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
        Encoder::FLAC => ".flac",
        Encoder::OGG | Encoder::OPUS => ".ogg",
    };

    let location = format!(
        "{}/{}-{}/{}{}",
        config.encode_path, disc.artist, disc.title, track.title, extension
    );
    //ensure folder exists
    std::fs::create_dir_all(
        Path::new(&location)
            .parent()
            .ok_or(anyhow!("failed to create folder".to_owned()))?,
    )?;
    let sink = ElementFactory::make("filesink").build()?;
    sink.set_property("location", location);

    let pipeline = Pipeline::new();
    match config.encoder {
        Encoder::MP3 => {
            let enc = ElementFactory::make("lamemp3enc").build()?;
            let quality = match config.quality {
                crate::data::Quality::Low => 9_f32,
                crate::data::Quality::Medium => 5_f32,
                crate::data::Quality::High => 0_f32,
            };
            enc.set_property("quality", quality);

            let tagsetter = &id3
                .dynamic_cast_ref::<TagSetter>()
                .ok_or(anyhow!("failed to cast"))?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &enc, &id3, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::OGG => {
            let convert = ElementFactory::make("audioconvert").build()?;
            let vorbis = ElementFactory::make("vorbisenc").build()?;
            let quality = match config.quality {
                crate::data::Quality::Low => 0.2_f32,
                crate::data::Quality::Medium => 0.5_f32,
                crate::data::Quality::High => 0.9_f32,
            };
            vorbis.set_property("quality", quality);
            let mux = ElementFactory::make("oggmux").build()?;

            let tagsetter = &vorbis
                .dynamic_cast_ref::<TagSetter>()
                .ok_or(anyhow!("failed to cast"))?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &convert, &vorbis, &mux, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::FLAC => {
            let enc = ElementFactory::make("flacenc").build()?;
            let elements = &[&extractor, &enc, &id3, &sink];
            let quality = match config.quality {
                crate::data::Quality::Low => "2",
                crate::data::Quality::Medium => "5",
                crate::data::Quality::High => "8",
            };
            enc.set_property_from_str("quality", quality);

            let tagsetter = &id3
                .dynamic_cast_ref::<TagSetter>()
                .ok_or(anyhow!("failed to cast"))?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
        Encoder::OPUS => {
            let convert = ElementFactory::make("audioconvert").build()?;
            let resample = ElementFactory::make("audioresample").build()?;
            let opus = ElementFactory::make("opusenc").build()?;
            let mux = ElementFactory::make("oggmux").build()?;

            let bitrate = match config.quality {
                crate::data::Quality::Low => 64_000_i32,
                crate::data::Quality::Medium => 128_000_i32,
                crate::data::Quality::High => 256_000_i32,
            };
            opus.set_property("bitrate", bitrate);
            let tagsetter = &opus
                .dynamic_cast_ref::<TagSetter>()
                .ok_or(anyhow!("failed to cast"))?;
            tagsetter.merge_tags(&tags, TagMergeMode::ReplaceAll);

            let elements = &[&extractor, &convert, &resample, &opus, &mux, &sink];
            pipeline.add_many(elements)?;
            Element::link_many(elements)?;
        }
    };

    Ok(pipeline)
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use gstreamer::{prelude::*, Element, ElementFactory, Pipeline};
    use serial_test::serial;
    use std::{
        env,
        fs::remove_file,
        path::Path,
        sync::{Arc, RwLock},
    };

    use super::extract_track;

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
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_mp3() -> Result<()> {
        gstreamer::init()?;
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/file_example_WAV_1MG.wav");

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
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_flac() -> Result<()> {
        gstreamer::init()?;
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
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
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_opus() -> Result<()> {
        gstreamer::init()?;
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
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
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    #[serial]
    pub fn test_ogg() -> Result<()> {
        gstreamer::init()?;
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/file_example_WAV_1MG.wav");
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
        remove_file(dest)?;
        Ok(())
    }
}
