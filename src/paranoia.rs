//! CD audio extraction using libcdio-paranoia (macOS only)
//!
//! This module provides CD audio reading with error correction via libcdio-paranoia,
//! replacing `GStreamer`'s cdda:// URI scheme which requires the unavailable cdparanoia plugin.
//!
//! On Linux, `GStreamer`'s cdda:// source is used instead (see ripper.rs).

use std::{
    ffi::CString,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anyhow::{Result, anyhow};
use gstreamer::{Buffer, Element, ElementFactory, FlowSuccess, prelude::*};
use gstreamer_app::AppSrc;
use gstreamer_audio::AudioInfo;
use libcdio_sys::{
    cdio_cddap_close, cdio_cddap_identify, cdio_cddap_open, cdio_cddap_track_firstsector,
    cdio_cddap_track_lastsector, cdio_paranoia_free, cdio_paranoia_init, cdio_paranoia_modeset,
    cdio_paranoia_read, cdio_paranoia_seek, cdrom_drive_t, cdrom_paranoia_t,
    paranoia_mode_t_PARANOIA_MODE_FULL,
};
use log::{debug, error, warn};

/// CD audio sector size in bytes (2352 = 588 samples * 2 channels * 2 bytes)
pub const CD_SECTOR_SIZE: usize = 2352;

/// CD audio sample rate
const CD_SAMPLE_RATE: i32 = 44100;

/// RAII wrapper for `cdrom_drive_t`
struct CddaHandle {
    ptr: *mut cdrom_drive_t,
}

impl CddaHandle {
    fn open(device: Option<&str>) -> Result<Self> {
        let c_device = device.map(CString::new).transpose()?;
        let device_ptr = c_device.as_ref().map_or(ptr::null(), |c| c.as_ptr());

        // cdio_cddap_identify creates the drive struct from a device path
        let ptr = unsafe { cdio_cddap_identify(device_ptr, 0, ptr::null_mut()) };

        if ptr.is_null() {
            return Err(anyhow!("Failed to identify CD drive for audio extraction"));
        }

        // cdio_cddap_open opens the drive for reading (returns 0 on success)
        let ret = unsafe { cdio_cddap_open(ptr) };
        if ret != 0 {
            unsafe { cdio_cddap_close(ptr) };
            return Err(anyhow!("Failed to open CD drive for audio reading"));
        }

        Ok(Self { ptr })
    }

    #[allow(clippy::cast_possible_truncation)] // CD track numbers are always ≤ 99
    fn track_first_sector(&self, track: u32) -> i32 {
        unsafe { cdio_cddap_track_firstsector(self.ptr, track as u8) }
    }

    #[allow(clippy::cast_possible_truncation)] // CD track numbers are always ≤ 99
    fn track_last_sector(&self, track: u32) -> i32 {
        unsafe { cdio_cddap_track_lastsector(self.ptr, track as u8) }
    }
}

impl Drop for CddaHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { cdio_cddap_close(self.ptr) };
        }
    }
}

/// RAII wrapper for `cdrom_paranoia_t`
struct ParanoiaHandle {
    ptr: *mut cdrom_paranoia_t,
}

impl ParanoiaHandle {
    fn new(drive: &CddaHandle) -> Result<Self> {
        let ptr = unsafe { cdio_paranoia_init(drive.ptr) };

        if ptr.is_null() {
            return Err(anyhow!("Failed to initialize paranoia"));
        }

        // Set full paranoia mode for maximum error correction
        #[allow(clippy::cast_possible_wrap)] // Known constant value
        unsafe {
            cdio_paranoia_modeset(ptr, paranoia_mode_t_PARANOIA_MODE_FULL as i32);
        }

        Ok(Self { ptr })
    }

    /// Seek to a specific sector
    fn seek(&self, sector: i32) {
        unsafe {
            cdio_paranoia_seek(self.ptr, sector, 0); // SEEK_SET = 0
        }
    }

    /// Read a sector with full paranoia error correction
    ///
    /// Returns owned data to avoid lifetime issues - the underlying C library
    /// uses a static buffer that's invalidated on each call.
    fn read_sector(&mut self) -> Option<Vec<i16>> {
        let sector = unsafe { cdio_paranoia_read(self.ptr, None) };

        if sector.is_null() {
            None
        } else {
            // Each sector is 588 stereo samples = 1176 i16 values
            let slice = unsafe { std::slice::from_raw_parts(sector, CD_SECTOR_SIZE / 2) };
            Some(slice.to_vec())
        }
    }
}

impl Drop for ParanoiaHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { cdio_paranoia_free(self.ptr) };
        }
    }
}

/// Information about a track for extraction
#[allow(dead_code)]
pub struct TrackInfo {
    pub first_sector: i32,
    pub last_sector: i32,
    pub total_sectors: u32,
}

/// Get track sector information
pub fn get_track_info(device: Option<&str>, track_number: u32) -> Result<TrackInfo> {
    let drive = CddaHandle::open(device)?;

    let first_sector = drive.track_first_sector(track_number);
    let last_sector = drive.track_last_sector(track_number);

    if first_sector < 0 || last_sector < 0 || last_sector < first_sector {
        return Err(anyhow!(
            "Invalid track sectors: first={first_sector}, last={last_sector}"
        ));
    }

    #[allow(clippy::cast_sign_loss)] // Checked above that values are non-negative
    let total_sectors = (last_sector - first_sector + 1) as u32;

    Ok(TrackInfo {
        first_sector,
        last_sector,
        total_sectors,
    })
}

/// Create a `GStreamer` appsrc element configured for CD audio
pub fn create_cd_appsrc(track_info: &TrackInfo) -> Result<Element> {
    let appsrc = ElementFactory::make("appsrc").name("cd_src").build()?;

    // CD audio format: 16-bit signed little-endian stereo at 44100 Hz
    let audio_info = AudioInfo::builder(
        gstreamer_audio::AudioFormat::S16le,
        CD_SAMPLE_RATE as u32,
        2,
    )
    .build()?;
    let caps = audio_info.to_caps()?;

    appsrc.set_property("caps", &caps);
    appsrc.set_property("format", gstreamer::Format::Time);
    appsrc.set_property("is-live", false);
    appsrc.set_property("block", true);

    // Set duration based on sector count
    // Duration = sectors * samples_per_sector / sample_rate
    // = sectors * 588 / 44100 seconds
    let duration_ns = u64::from(track_info.total_sectors) * 588 * 1_000_000_000 / 44100;
    #[allow(clippy::cast_possible_wrap)] // CD tracks are never long enough to overflow i64
    appsrc.set_property("duration", duration_ns as i64);

    Ok(appsrc)
}

/// Context for the extraction thread
struct ExtractionContext {
    device: Option<String>,
    track_number: u32,
    appsrc: AppSrc,
    abort: Arc<AtomicBool>,
    progress_sectors: Arc<std::sync::atomic::AtomicU32>,
    total_sectors: u32,
}

/// Start reading CD audio in a background thread and push to appsrc
pub fn start_extraction(
    device: Option<&str>,
    track_number: u32,
    appsrc: &Element,
    abort: Arc<AtomicBool>,
) -> Result<(
    thread::JoinHandle<Result<()>>,
    Arc<std::sync::atomic::AtomicU32>,
    u32,
)> {
    let track_info = get_track_info(device, track_number)?;

    let appsrc = appsrc
        .clone()
        .dynamic_cast::<AppSrc>()
        .map_err(|_| anyhow!("Element is not an AppSrc"))?;

    let progress_sectors = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let progress_clone = progress_sectors.clone();
    let total_sectors = track_info.total_sectors;

    let ctx = ExtractionContext {
        device: device.map(String::from),
        track_number,
        appsrc,
        abort,
        progress_sectors: progress_clone,
        total_sectors,
    };

    let handle = thread::spawn(move || extraction_thread(ctx));

    Ok((handle, progress_sectors, total_sectors))
}

/// The actual extraction thread function
#[allow(clippy::needless_pass_by_value)] // Context is moved into the thread
fn extraction_thread(ctx: ExtractionContext) -> Result<()> {
    debug!(
        "Starting extraction of track {} ({} sectors)",
        ctx.track_number, ctx.total_sectors
    );

    let drive = CddaHandle::open(ctx.device.as_deref())?;
    let mut paranoia = ParanoiaHandle::new(&drive)?;

    // Seek to the first sector of the track
    let first_sector = drive.track_first_sector(ctx.track_number);
    paranoia.seek(first_sector);

    let mut sectors_read = 0u32;

    while sectors_read < ctx.total_sectors {
        // Check for abort
        if ctx.abort.load(Ordering::Relaxed) {
            debug!("Extraction aborted by user");
            return Ok(());
        }

        // Read a sector
        let samples = if let Some(s) = paranoia.read_sector() {
            s
        } else {
            #[allow(clippy::cast_possible_wrap)] // sectors_read is always small
            let sector_num = first_sector + sectors_read as i32;
            error!("Failed to read sector {sector_num}");
            // Continue with silent sector on read error
            warn!("Inserting silent sector for failed read");
            vec![0i16; CD_SECTOR_SIZE / 2]
        };

        // Convert to bytes and create a GStreamer buffer
        let bytes: Vec<u8> = samples.into_iter().flat_map(i16::to_le_bytes).collect();

        let mut buffer = Buffer::from_slice(bytes);

        // Set buffer timestamps for proper progress tracking
        {
            let buffer_ref = buffer.get_mut().unwrap();
            let pts = u64::from(sectors_read) * 588 * 1_000_000_000 / 44100;
            let duration = 588 * 1_000_000_000 / 44100;
            buffer_ref.set_pts(gstreamer::ClockTime::from_nseconds(pts));
            buffer_ref.set_duration(gstreamer::ClockTime::from_nseconds(duration));
        }

        // Push to appsrc
        match ctx.appsrc.push_buffer(buffer) {
            Ok(FlowSuccess::Ok) => {}
            Ok(_) => {
                debug!("AppSrc returned non-Ok flow");
                break;
            }
            Err(e) => {
                error!("Failed to push buffer to appsrc: {e:?}");
                break;
            }
        }

        sectors_read += 1;
        ctx.progress_sectors.store(sectors_read, Ordering::Relaxed);
    }

    debug!("Extraction complete, sending EOS");
    let _ = ctx.appsrc.end_of_stream();

    Ok(())
}

/// Calculate extraction progress as a percentage
pub fn calculate_extraction_progress(
    progress_sectors: &Arc<std::sync::atomic::AtomicU32>,
    total_sectors: u32,
) -> f64 {
    if total_sectors == 0 {
        return 0.0;
    }
    let current = progress_sectors.load(Ordering::Relaxed);
    (f64::from(current) / f64::from(total_sectors)) * 100.0
}
