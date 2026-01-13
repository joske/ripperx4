#[cfg(target_os = "macos")]
use crate::paranoia::{
    calculate_extraction_progress, create_cd_appsrc, get_track_info, start_extraction,
};
use crate::{
    data::{Config, Disc, Encoder, Track},
    util::read_config,
};
use anyhow::{Result, anyhow};
use async_channel::Sender;
#[cfg(target_os = "macos")]
use discid::DiscId;
use glib::ControlFlow;
use gstreamer::{
    ClockTime, Element, ElementFactory, MessageView, Pipeline, State, TagList, TagMergeMode,
    TagSetter,
    bus::BusWatchGuard,
    glib,
    glib::MainLoop,
    prelude::*,
    tags::{Album, Artist, Composer, Date, Duration, Title, TrackNumber},
};
#[cfg(not(target_os = "macos"))]
use gstreamer::{Format, GenericFormattedValue, URIType, format::Percent};
use log::{debug, error};
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU32;
use std::{
    fmt::Write,
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// Encapsulates the state for extracting a single track.
///
/// This struct manages the coordination between the `GStreamer` pipeline,
/// progress updates, and user cancellation. It uses atomic booleans for
/// lock-free access to state flags from multiple contexts (bus watch,
/// progress timer, and main loop).
struct TrackExtractor {
    /// Whether the extraction is still in progress
    working: Arc<AtomicBool>,
    /// Whether the user requested cancellation
    aborted: Arc<AtomicBool>,
    /// Last error message from `GStreamer`, if any
    last_error: Arc<RwLock<Option<String>>>,
    /// Channel to send status updates to the UI
    status: Sender<String>,
    /// External flag indicating if ripping should continue (shared across all tracks)
    ripping: Arc<RwLock<bool>>,
    /// Progress from paranoia extraction (sectors read) - macOS only
    #[cfg(target_os = "macos")]
    progress_sectors: Option<Arc<AtomicU32>>,
    /// Total sectors for the track - macOS only
    #[cfg(target_os = "macos")]
    total_sectors: u32,
}

impl TrackExtractor {
    fn new(status: Sender<String>, ripping: Arc<RwLock<bool>>) -> Self {
        Self {
            working: Arc::new(AtomicBool::new(true)),
            aborted: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(RwLock::new(None)),
            status,
            ripping,
            #[cfg(target_os = "macos")]
            progress_sectors: None,
            #[cfg(target_os = "macos")]
            total_sectors: 0,
        }
    }

    #[cfg(target_os = "macos")]
    fn set_progress_tracking(&mut self, progress_sectors: Arc<AtomicU32>, total_sectors: u32) {
        self.progress_sectors = Some(progress_sectors);
        self.total_sectors = total_sectors;
    }

    fn was_aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }

    fn take_error(&self) -> Option<String> {
        self.last_error.read().ok().and_then(|e| e.clone())
    }
}

/// Check which output files already exist for tracks marked for ripping
pub fn check_existing_files(disc: &Disc) -> Vec<String> {
    let config: Config = read_config();
    disc.tracks
        .iter()
        .filter(|t| t.rip)
        .filter_map(|track| {
            let path = format_output_path(&config, disc, track);
            if Path::new(&path).exists() {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

/// Create a fake ripped file (for testing without a CD)
fn create_fake_file(config: &Config, disc: &Disc, track: &Track, overwrite: bool) -> Result<()> {
    let path = build_output_path(config, disc, track, overwrite)?;
    debug!("Creating fake file: {path}");
    // Create an empty file with the correct extension
    std::fs::write(&path, [])?;
    Ok(())
}

/// Simulate ripping by creating empty files (for testing)
fn fake_extract(
    disc: &Disc,
    status: &Sender<String>,
    ripping: &Arc<RwLock<bool>>,
    overwrite: bool,
) -> Result<()> {
    let config = read_config();
    for track in &disc.tracks {
        if !is_ripping(ripping) {
            debug!("Fake ripping aborted by user");
            break;
        }
        if track.rip {
            let _ = status.send_blocking(format!("Simulating rip of {}", track.title));
            create_fake_file(&config, disc, track, overwrite)?;
            // Small delay to simulate ripping
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    Ok(())
}

/// Extract/Rip a `Disc` to the configured format
#[cfg(target_os = "macos")]
pub fn extract(
    disc: &Disc,
    status: &Sender<String>,
    ripping: &Arc<RwLock<bool>>,
    overwrite: bool,
) -> Result<()> {
    let config = read_config();
    if config.fake_cdrom {
        return fake_extract(disc, status, ripping, overwrite);
    }

    for track in &disc.tracks {
        if !is_ripping(ripping) {
            debug!("Ripping aborted by user");
            break;
        }
        if track.rip {
            let (pipeline, source) = create_pipeline(track, disc, overwrite)?;
            extract_track(
                &pipeline,
                &source,
                track.number,
                &track.title,
                status,
                ripping.clone(),
            )?;
        }
    }
    Ok(())
}

/// Extract/Rip a `Disc` to the configured format (Linux - uses cdda://)
#[cfg(not(target_os = "macos"))]
pub fn extract(
    disc: &Disc,
    status: &Sender<String>,
    ripping: &Arc<RwLock<bool>>,
    overwrite: bool,
) -> Result<()> {
    let config = read_config();
    if config.fake_cdrom {
        return fake_extract(disc, status, ripping, overwrite);
    }

    for track in &disc.tracks {
        if !is_ripping(ripping) {
            debug!("Ripping aborted by user");
            break;
        }
        if track.rip {
            let pipeline = create_pipeline(track, disc, overwrite)?;
            extract_track(&pipeline, &track.title, status, ripping.clone())?;
        }
    }
    Ok(())
}

/// Check if we should continue ripping
fn is_ripping(ripping: &Arc<RwLock<bool>>) -> bool {
    ripping.read().map(|r| *r).unwrap_or(false)
}

/// Run a pipeline to completion, handling progress and cancellation (macOS version)
#[cfg(target_os = "macos")]
fn run_pipeline(
    pipeline: &Pipeline,
    title: &str,
    status: &Sender<String>,
    ripping: Arc<RwLock<bool>>,
    progress_info: Option<(Arc<AtomicU32>, u32)>,
) -> Result<()> {
    let status_message = format!("Encoding {title}");
    let _ = status.send_blocking(status_message.clone());

    let mut extractor = TrackExtractor::new(status.clone(), ripping);
    let main_loop = MainLoop::new(None, false);

    if let Some((progress_sectors, total_sectors)) = progress_info {
        extractor.set_progress_tracking(progress_sectors, total_sectors);
    }

    pipeline.set_state(State::Playing)?;

    start_progress_updates(
        &extractor,
        status_message,
        pipeline.clone(),
        main_loop.clone(),
    );
    let _bus_watch = setup_bus_watch(&extractor, pipeline, main_loop.clone())?;

    main_loop.run();

    if let Some(msg) = extractor.take_error() {
        return Err(anyhow!(msg));
    }
    if extractor.was_aborted() {
        debug!("Encoding {title} aborted by user request");
        return Err(anyhow!("Ripping aborted by user"));
    }
    debug!("Finished encoding {title}");
    Ok(())
}

/// Rip one track using the provided pipeline with paranoia extraction (macOS)
#[cfg(target_os = "macos")]
fn extract_track(
    pipeline: &Pipeline,
    source: &Element,
    track_number: u32,
    title: &str,
    status: &Sender<String>,
    ripping: Arc<RwLock<bool>>,
) -> Result<()> {
    // Start the paranoia extraction thread
    let device = DiscId::default_device();
    let (extraction_handle, progress_sectors, total_sectors) = start_extraction(
        Some(&device),
        track_number,
        source,
        Arc::new(AtomicBool::new(false)), // abort flag managed separately
    )?;

    let result = run_pipeline(
        pipeline,
        title,
        status,
        ripping,
        Some((progress_sectors, total_sectors)),
    );

    // Wait for the extraction thread to finish
    if let Err(e) = extraction_handle.join() {
        error!("Extraction thread panicked: {e:?}");
    }

    result
}

/// Rip one track using the provided pipeline (Linux - uses cdda://)
#[cfg(not(target_os = "macos"))]
fn extract_track(
    pipeline: &Pipeline,
    title: &str,
    status: &Sender<String>,
    ripping: Arc<RwLock<bool>>,
) -> Result<()> {
    let status_message = format!("Encoding {title}");
    let _ = status.send_blocking(status_message.clone());

    let extractor = TrackExtractor::new(status.clone(), ripping);
    let main_loop = MainLoop::new(None, false);

    pipeline.set_state(State::Playing)?;

    start_progress_updates(
        &extractor,
        status_message,
        pipeline.clone(),
        main_loop.clone(),
    );
    let _bus_watch = setup_bus_watch(&extractor, pipeline, main_loop.clone())?;

    main_loop.run();

    if let Some(msg) = extractor.take_error() {
        return Err(anyhow!(msg));
    }
    if extractor.was_aborted() {
        debug!("Encoding {title} aborted by user request");
        return Err(anyhow!("Ripping aborted by user"));
    }
    debug!("Finished encoding {title}");
    Ok(())
}

/// Set up the `GStreamer` bus watch to handle EOS and error messages.
/// Returns the guard that must be kept alive until the main loop exits.
fn setup_bus_watch(
    extractor: &TrackExtractor,
    pipeline: &Pipeline,
    main_loop: MainLoop,
) -> Result<BusWatchGuard> {
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("Pipeline has no bus"))?;

    let working = extractor.working.clone();
    let last_error = extractor.last_error.clone();
    let status = extractor.status.clone();
    let pipeline = pipeline.clone();

    let guard = bus.add_watch(move |_, msg| {
        match msg.view() {
            MessageView::Eos(..) => {
                debug!("End of stream");
                working.store(false, Ordering::Relaxed);
                let _ = pipeline.set_state(State::Null);
                main_loop.quit();
            }
            MessageView::Error(err) => {
                let _ = status.send_blocking("aborted".to_owned());
                working.store(false, Ordering::Relaxed);

                let error_msg = format!(
                    "GStreamer error from {:?}: {} ({:?})",
                    err.src().map(GstObjectExt::path_string),
                    err.error(),
                    err.debug()
                );
                error!("{error_msg}");

                if let Ok(mut e) = last_error.write() {
                    *e = Some(error_msg);
                }

                let _ = pipeline.set_state(State::Null);
                main_loop.quit();
            }
            _ => (),
        }
        ControlFlow::Continue
    })?;

    Ok(guard)
}

/// Start periodic progress updates (macOS - uses paranoia sector progress)
#[cfg(target_os = "macos")]
fn start_progress_updates(
    extractor: &TrackExtractor,
    status_message: String,
    pipeline: Pipeline,
    main_loop: MainLoop,
) {
    let working = extractor.working.clone();
    let aborted = extractor.aborted.clone();
    let ripping = extractor.ripping.clone();
    let status = extractor.status.clone();
    let progress_sectors = extractor.progress_sectors.clone();
    let total_sectors = extractor.total_sectors;

    glib::timeout_add(std::time::Duration::from_millis(500), move || {
        if !working.load(Ordering::Relaxed) {
            return ControlFlow::Break;
        }

        if !ripping.read().map(|r| *r).unwrap_or(false) {
            working.store(false, Ordering::Relaxed);
            aborted.store(true, Ordering::Relaxed);
            let _ = pipeline.set_state(State::Null);
            main_loop.quit();
            return ControlFlow::Break;
        }

        let percent = if let Some(ref progress) = progress_sectors {
            calculate_extraction_progress(progress, total_sectors)
        } else {
            0.0
        };
        let msg = format!("{status_message} : {percent:.0} %");
        let _ = status.send_blocking(msg);

        ControlFlow::Continue
    });
}

/// Start periodic progress updates (Linux - uses `GStreamer` pipeline progress)
#[cfg(not(target_os = "macos"))]
fn start_progress_updates(
    extractor: &TrackExtractor,
    status_message: String,
    pipeline: Pipeline,
    main_loop: MainLoop,
) {
    let working = extractor.working.clone();
    let aborted = extractor.aborted.clone();
    let ripping = extractor.ripping.clone();
    let status = extractor.status.clone();

    glib::timeout_add(std::time::Duration::from_millis(500), move || {
        if !working.load(Ordering::Relaxed) {
            return ControlFlow::Break;
        }

        if !ripping.read().map(|r| *r).unwrap_or(false) {
            working.store(false, Ordering::Relaxed);
            aborted.store(true, Ordering::Relaxed);
            let _ = pipeline.set_state(State::Null);
            main_loop.quit();
            return ControlFlow::Break;
        }

        let percent = calculate_progress(&pipeline);
        let msg = format!("{status_message} : {percent:.0} %");
        let _ = status.send_blocking(msg);

        ControlFlow::Continue
    });
}

/// Calculate pipeline progress as percentage (Linux only)
#[cfg(not(target_os = "macos"))]
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

/// Create a `GStreamer` pipeline for encoding a track (macOS - uses appsrc)
#[cfg(target_os = "macos")]
fn create_pipeline(track: &Track, disc: &Disc, overwrite: bool) -> Result<(Pipeline, Element)> {
    let config: Config = read_config();

    let device = DiscId::default_device();
    let track_info = get_track_info(Some(&device), track.number)?;
    let source = create_cd_appsrc(&track_info)?;

    let tags = build_tags(track, disc)?;
    let output_path = build_output_path(&config, disc, track, overwrite)?;
    let sink = create_file_sink(&output_path)?;

    let pipeline = Pipeline::new();

    match config.encoder {
        Encoder::MP3 => build_mp3_pipeline(&pipeline, source.clone(), sink, &tags, config.quality)?,
        Encoder::OGG => build_ogg_pipeline(&pipeline, source.clone(), sink, &tags, config.quality)?,
        Encoder::FLAC => {
            build_flac_pipeline(&pipeline, source.clone(), sink, &tags, config.quality)?;
        }
        Encoder::OPUS => {
            build_opus_pipeline(&pipeline, source.clone(), sink, &tags, config.quality)?;
        }
        Encoder::WAV => build_wav_pipeline(&pipeline, source.clone(), sink)?,
    }

    Ok((pipeline, source))
}

/// Create a `GStreamer` pipeline for encoding a track (Linux - uses cdda://)
#[cfg(not(target_os = "macos"))]
fn create_pipeline(track: &Track, disc: &Disc, overwrite: bool) -> Result<Pipeline> {
    let config: Config = read_config();

    let source = create_cd_source(track.number)?;
    let tags = build_tags(track, disc)?;
    let output_path = build_output_path(&config, disc, track, overwrite)?;
    let sink = create_file_sink(&output_path)?;

    let pipeline = Pipeline::new();

    match config.encoder {
        Encoder::MP3 => build_mp3_pipeline(&pipeline, source, sink, &tags, config.quality)?,
        Encoder::OGG => build_ogg_pipeline(&pipeline, source, sink, &tags, config.quality)?,
        Encoder::FLAC => build_flac_pipeline(&pipeline, source, sink, &tags, config.quality)?,
        Encoder::OPUS => build_opus_pipeline(&pipeline, source, sink, &tags, config.quality)?,
        Encoder::WAV => build_wav_pipeline(&pipeline, source, sink)?,
    }

    Ok(pipeline)
}

/// Create the CD audio source element using cdda:// (Linux only)
#[cfg(not(target_os = "macos"))]
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

/// Apply a pattern template, substituting placeholders with sanitized values
fn apply_pattern(pattern: &str, disc: &Disc, track: &Track) -> String {
    pattern
        .replace("{artist}", &sanitize_path_component(&disc.artist))
        .replace("{album}", &sanitize_path_component(&disc.title))
        .replace("{title}", &sanitize_path_component(&track.title))
        .replace("{number}", &format!("{:02}", track.number))
        .replace(
            "{year}",
            &disc.year.map_or(String::new(), |y| y.to_string()),
        )
        .replace(
            "{genre}",
            &sanitize_path_component(&disc.genre.clone().unwrap_or_default()),
        )
}

/// Format the output file path without any checks
fn format_output_path(config: &Config, disc: &Disc, track: &Track) -> String {
    let extension = config.encoder.file_extension();
    let pattern = config.file_pattern.template(&config.custom_pattern);
    let relative_path = apply_pattern(pattern, disc, track);
    format!("{}/{}{}", config.encode_path, relative_path, extension)
}

/// Extract the filename portion from a pattern (everything after the last `/`)
fn get_filename_pattern(pattern: &str) -> &str {
    pattern.rsplit('/').next().unwrap_or(pattern)
}

/// Generate M3U playlist content for the given disc using the configured pattern
fn generate_playlist_content(disc: &Disc, extension: &str, pattern: &str) -> String {
    let filename_pattern = get_filename_pattern(pattern);
    let mut content = String::from("#EXTM3U\n");
    for track in &disc.tracks {
        if track.rip {
            let filename = format!(
                "{}{}",
                apply_pattern(filename_pattern, disc, track),
                extension
            );
            // #EXTINF:duration,Artist - Title
            let _ = writeln!(
                content,
                "#EXTINF:{},{} - {}\n{}",
                track.duration, disc.artist, track.title, filename
            );
        }
    }
    content
}

/// Create an M3U playlist file for the ripped tracks
pub fn create_playlist(disc: &Disc) -> Result<()> {
    let config = read_config();
    let extension = config.encoder.file_extension();
    let pattern = config.file_pattern.template(&config.custom_pattern);

    // Use the first track to determine the album directory
    let first_track = disc
        .tracks
        .first()
        .ok_or_else(|| anyhow!("No tracks to create playlist for"))?;
    let sample_path = format_output_path(&config, disc, first_track);
    let album_dir = Path::new(&sample_path)
        .parent()
        .ok_or_else(|| anyhow!("Invalid output path"))?;

    let album = sanitize_path_component(&disc.title);
    let playlist_path = album_dir.join(format!("{album}.m3u"));

    let content = generate_playlist_content(disc, extension, pattern);
    std::fs::write(&playlist_path, content)?;
    debug!("Created playlist: {}", playlist_path.display());
    Ok(())
}

/// Open the album folder in the system file manager
pub fn open_album_folder(disc: &Disc) -> Result<()> {
    let config = read_config();

    // Use the first track to determine the album directory
    let first_track = disc
        .tracks
        .first()
        .ok_or_else(|| anyhow!("No tracks to open folder for"))?;
    let sample_path = format_output_path(&config, disc, first_track);
    let album_dir = Path::new(&sample_path)
        .parent()
        .ok_or_else(|| anyhow!("Invalid output path"))?;

    debug!("Opening folder: {}", album_dir.display());

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(album_dir).spawn()?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new("xdg-open")
            .arg(album_dir)
            .spawn()?;
    }

    Ok(())
}

/// Build the output file path and ensure directory exists
fn build_output_path(
    config: &Config,
    disc: &Disc,
    track: &Track,
    overwrite: bool,
) -> Result<String> {
    let path = format_output_path(config, disc, track);

    if !overwrite && Path::new(&path).exists() {
        return Err(anyhow!("File already exists: {path}"));
    }

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

#[allow(clippy::needless_pass_by_value)] // Elements are consumed by pipeline
fn build_wav_pipeline(pipeline: &Pipeline, source: Element, sink: Element) -> Result<()> {
    let encoder = ElementFactory::make("wavenc").build()?;
    link_pipeline(pipeline, &[&source, &encoder, &sink])
}

#[cfg(test)]
mod test {
    use anyhow::{Result, anyhow};
    use glib::{ControlFlow, MainLoop};
    use gstreamer::{
        Bin, Element, ElementFactory, GhostPad, MessageView, Pipeline, State, prelude::*,
    };
    use std::{
        env,
        fs::{File, remove_file},
        io::Read,
        path::Path,
        sync::{
            Arc, RwLock,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::{
        build_flac_pipeline, build_mp3_pipeline, build_ogg_pipeline, build_opus_pipeline,
        build_wav_pipeline, generate_playlist_content, sanitize_path_component,
    };
    use crate::data::{Disc, Quality, Track};

    /// Test-only version of `run_pipeline` that works on all platforms
    fn run_pipeline(
        pipeline: &Pipeline,
        title: &str,
        status: &async_channel::Sender<String>,
        ripping: &Arc<RwLock<bool>>,
    ) -> Result<()> {
        let status_message = format!("Encoding {title}");
        let _ = status.send_blocking(status_message.clone());

        let working = Arc::new(AtomicBool::new(true));
        let aborted = Arc::new(AtomicBool::new(false));
        let main_loop = MainLoop::new(None, false);

        pipeline.set_state(State::Playing)?;

        // Progress updates
        {
            let working = working.clone();
            let aborted = aborted.clone();
            let ripping = ripping.clone();
            let pipeline = pipeline.clone();
            let main_loop = main_loop.clone();

            glib::timeout_add(std::time::Duration::from_millis(500), move || {
                if !working.load(Ordering::Relaxed) {
                    return ControlFlow::Break;
                }
                if !ripping.read().map(|r| *r).unwrap_or(false) {
                    working.store(false, Ordering::Relaxed);
                    aborted.store(true, Ordering::Relaxed);
                    let _ = pipeline.set_state(State::Null);
                    main_loop.quit();
                    return ControlFlow::Break;
                }
                ControlFlow::Continue
            });
        }

        // Bus watch
        let bus = pipeline
            .bus()
            .ok_or_else(|| anyhow!("Pipeline has no bus"))?;
        let working_bus = working.clone();
        let pipeline_bus = pipeline.clone();
        let main_loop_bus = main_loop.clone();
        let last_error: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
        let last_error_bus = last_error.clone();

        let _guard = bus.add_watch(move |_, msg| {
            match msg.view() {
                MessageView::Eos(..) => {
                    working_bus.store(false, Ordering::Relaxed);
                    let _ = pipeline_bus.set_state(State::Null);
                    main_loop_bus.quit();
                }
                MessageView::Error(err) => {
                    working_bus.store(false, Ordering::Relaxed);
                    let error_msg = format!("GStreamer error: {} ({:?})", err.error(), err.debug());
                    if let Ok(mut e) = last_error_bus.write() {
                        *e = Some(error_msg);
                    }
                    let _ = pipeline_bus.set_state(State::Null);
                    main_loop_bus.quit();
                }
                _ => (),
            }
            ControlFlow::Continue
        })?;

        main_loop.run();

        if let Some(msg) = last_error.read().ok().and_then(|e| e.clone()) {
            return Err(anyhow!(msg));
        }
        if aborted.load(Ordering::Relaxed) {
            return Err(anyhow!("Ripping aborted by user"));
        }
        Ok(())
    }

    // ==================== sanitize_path_component tests ====================

    #[test]
    fn sanitize_path_component_removes_slashes() {
        assert_eq!(sanitize_path_component("AC/DC"), "AC_DC");
    }

    #[test]
    fn sanitize_path_component_removes_backslashes() {
        assert_eq!(sanitize_path_component("Back\\Slash"), "Back_Slash");
    }

    #[test]
    fn sanitize_path_component_handles_empty_string() {
        assert_eq!(sanitize_path_component(""), "Unknown");
    }

    #[test]
    fn sanitize_path_component_handles_only_whitespace() {
        assert_eq!(sanitize_path_component("   "), "Unknown");
    }

    #[test]
    fn sanitize_path_component_handles_only_invalid_chars() {
        assert_eq!(sanitize_path_component("///"), "___");
    }

    #[test]
    fn sanitize_path_component_preserves_alphanumeric() {
        assert_eq!(sanitize_path_component("Track01"), "Track01");
    }

    #[test]
    fn sanitize_path_component_preserves_spaces_dots_underscores_dashes() {
        assert_eq!(
            sanitize_path_component("My Song - Part 1.5_remix"),
            "My Song - Part 1.5_remix"
        );
    }

    #[test]
    fn sanitize_path_component_replaces_special_chars() {
        assert_eq!(
            sanitize_path_component("Song: The <Best>?"),
            "Song_ The _Best__"
        );
    }

    #[test]
    fn sanitize_path_component_trims_whitespace() {
        assert_eq!(sanitize_path_component("  Trimmed  "), "Trimmed");
    }

    #[test]
    fn sanitize_path_component_replaces_unicode() {
        // Non-ASCII chars should be replaced with underscore
        assert_eq!(sanitize_path_component("Müsic"), "M_sic");
        assert_eq!(sanitize_path_component("日本語"), "___");
    }

    #[test]
    fn build_tags_includes_all_metadata() -> Result<()> {
        gstreamer::init()?;

        let track = Track {
            number: 5,
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            duration: 180,
            composer: Some("Test Composer".to_string()),
            rip: true,
        };

        let disc = Disc {
            title: "Test Album".to_string(),
            artist: "Album Artist".to_string(),
            year: Some(2023),
            genre: None,
            tracks: vec![],
        };

        let tags = super::build_tags(&track, &disc)?;

        // Verify tags were created - n_tags returns the number of distinct tag names
        assert!(tags.n_tags() > 0);
        Ok(())
    }

    #[test]
    fn build_tags_handles_missing_optional_fields() -> Result<()> {
        gstreamer::init()?;

        let track = Track {
            number: 1,
            title: "No Composer Track".to_string(),
            artist: "Artist".to_string(),
            duration: 120,
            composer: None,
            rip: true,
        };

        let disc = Disc {
            title: "No Year Album".to_string(),
            artist: "Artist".to_string(),
            year: None,
            genre: None,
            tracks: vec![],
        };

        let tags = super::build_tags(&track, &disc)?;
        assert!(tags.n_tags() > 0);
        Ok(())
    }

    fn test_pcm_path() -> Result<String> {
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/test_audio.pcm");
        Ok(path)
    }

    /// Create a source bin that reads raw PCM and outputs audio
    /// This simulates what cdda:// does when reading from a CD
    fn create_test_source() -> Result<Element> {
        let bin = Bin::new();

        let filesrc = ElementFactory::make("filesrc").build()?;
        filesrc.set_property("location", test_pcm_path()?);

        let parse = ElementFactory::make("rawaudioparse").build()?;
        // CD audio format: 44100Hz, stereo, 16-bit signed little-endian
        parse.set_property_from_str("format", "pcm");
        parse.set_property_from_str("pcm-format", "s16le");
        parse.set_property("sample-rate", 44100i32);
        parse.set_property("num-channels", 2i32);

        bin.add_many([&filesrc, &parse])?;
        filesrc.link(&parse)?;

        // Create ghost pad to expose the audio output
        let src_pad = parse.static_pad("src").ok_or(anyhow!("No src pad"))?;
        let ghost_pad = GhostPad::with_target(&src_pad)?;
        ghost_pad.set_active(true)?;
        bin.add_pad(&ghost_pad)?;

        Ok(bin.upcast())
    }

    struct TestPipeline {
        pipeline: Pipeline,
        source: Element,
        sink: Element,
        tags: gstreamer::TagList,
    }

    fn setup_test_pipeline(dest: &str) -> Result<TestPipeline> {
        gstreamer::init()?;
        let pipeline = Pipeline::new();
        let source = create_test_source()?;
        let sink = ElementFactory::make("filesink").build()?;
        sink.set_property("location", dest);
        let tags = gstreamer::TagList::new();
        Ok(TestPipeline {
            pipeline,
            source,
            sink,
            tags,
        })
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
            // WAV: "RIFF....WAVE"
            [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x41, 0x56, 0x45] => FileType::Wav,
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
        Wav,
    }

    #[test]
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
        let result = run_pipeline(&pipeline, "track", &tx, &ripping);
        // Pipeline fails because filesrc->filesink is invalid (incompatible elements)
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    pub fn test_mp3() -> Result<()> {
        let dest = "/tmp/test_audio.mp3";
        let t = setup_test_pipeline(dest)?;

        build_mp3_pipeline(&t.pipeline, t.source, t.sink, &t.tags, Quality::Medium)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        run_pipeline(&t.pipeline, "track", &tx, &ripping)?;

        assert!(Path::new(dest).exists());
        verify_file_type(dest, &FileType::Mp3)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    pub fn test_flac() -> Result<()> {
        let dest = "/tmp/test_audio.flac";
        let t = setup_test_pipeline(dest)?;

        build_flac_pipeline(&t.pipeline, t.source, t.sink, &t.tags, Quality::Medium)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        run_pipeline(&t.pipeline, "track", &tx, &ripping)?;

        assert!(Path::new(dest).exists());
        verify_file_type(dest, &FileType::Flac)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    pub fn test_opus() -> Result<()> {
        let dest = "/tmp/test_audio_opus.ogg";
        let t = setup_test_pipeline(dest)?;

        build_opus_pipeline(&t.pipeline, t.source, t.sink, &t.tags, Quality::Medium)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        run_pipeline(&t.pipeline, "track", &tx, &ripping)?;

        assert!(Path::new(dest).exists());
        verify_file_type(dest, &FileType::Ogg)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    pub fn test_ogg() -> Result<()> {
        let dest = "/tmp/test_audio.ogg";
        let t = setup_test_pipeline(dest)?;

        build_ogg_pipeline(&t.pipeline, t.source, t.sink, &t.tags, Quality::Medium)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        run_pipeline(&t.pipeline, "track", &tx, &ripping)?;

        assert!(Path::new(dest).exists());
        verify_file_type(dest, &FileType::Ogg)?;
        remove_file(dest)?;
        Ok(())
    }

    #[test]
    pub fn test_wav() -> Result<()> {
        let dest = "/tmp/test_audio.wav";
        let t = setup_test_pipeline(dest)?;

        build_wav_pipeline(&t.pipeline, t.source, t.sink)?;

        let (tx, _rx) = async_channel::unbounded();
        let ripping = Arc::new(RwLock::new(true));
        run_pipeline(&t.pipeline, "track", &tx, &ripping)?;

        assert!(Path::new(dest).exists());
        verify_file_type(dest, &FileType::Wav)?;
        remove_file(dest)?;
        Ok(())
    }

    // ==================== Playlist tests ====================

    #[test]
    fn generate_playlist_creates_valid_m3u() {
        let disc = Disc {
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            year: Some(2024),
            genre: Some("Rock".to_string()),
            tracks: vec![
                Track {
                    number: 1,
                    title: "First Song".to_string(),
                    artist: "Test Artist".to_string(),
                    duration: 180,
                    composer: None,
                    rip: true,
                },
                Track {
                    number: 2,
                    title: "Second Song".to_string(),
                    artist: "Test Artist".to_string(),
                    duration: 240,
                    composer: None,
                    rip: true,
                },
                Track {
                    number: 3,
                    title: "Skipped Song".to_string(),
                    artist: "Test Artist".to_string(),
                    duration: 200,
                    composer: None,
                    rip: false, // This track should be excluded
                },
            ],
        };

        let pattern = "{artist}/{album}/{number} - {title}";
        let content = generate_playlist_content(&disc, ".mp3", pattern);

        assert!(content.starts_with("#EXTM3U\n"));
        assert!(content.contains("#EXTINF:180,Test Artist - First Song"));
        assert!(content.contains("01 - First Song.mp3"));
        assert!(content.contains("#EXTINF:240,Test Artist - Second Song"));
        assert!(content.contains("02 - Second Song.mp3"));
        // Track 3 should NOT be in the playlist (rip=false)
        assert!(!content.contains("Skipped Song"));
    }

    #[test]
    fn generate_playlist_handles_empty_tracks() {
        let disc = Disc {
            title: "Empty Album".to_string(),
            artist: "Artist".to_string(),
            year: None,
            genre: None,
            tracks: vec![],
        };

        let pattern = "{artist}/{album}/{number} - {title}";
        let content = generate_playlist_content(&disc, ".flac", pattern);

        assert_eq!(content, "#EXTM3U\n");
    }

    #[test]
    fn generate_playlist_sanitizes_filenames() {
        let disc = Disc {
            title: "Album".to_string(),
            artist: "AC/DC".to_string(),
            year: None,
            genre: None,
            tracks: vec![Track {
                number: 1,
                title: "Highway to Hell".to_string(),
                artist: "AC/DC".to_string(),
                duration: 208,
                composer: None,
                rip: true,
            }],
        };

        let pattern = "{artist}/{album}/{number} - {title}";
        let content = generate_playlist_content(&disc, ".mp3", pattern);

        // Filename should be sanitized (no slashes)
        assert!(content.contains("01 - Highway to Hell.mp3"));
        // But EXTINF should have original artist name
        assert!(content.contains("#EXTINF:208,AC/DC - Highway to Hell"));
    }
}
