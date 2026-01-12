use std::ffi::{CStr, CString};
use std::ptr;

use discid::DiscId;
use libcdio_sys::{
    cdio_destroy, cdio_get_cdtext, cdio_get_first_track_num, cdio_get_num_tracks, cdio_open,
    cdtext_field_t_CDTEXT_FIELD_COMPOSER, cdtext_field_t_CDTEXT_FIELD_GENRE,
    cdtext_field_t_CDTEXT_FIELD_PERFORMER, cdtext_field_t_CDTEXT_FIELD_TITLE, cdtext_get_const,
    driver_id_t_DRIVER_UNKNOWN, CdIo_t,
};
use log::{debug, info};

use crate::data::{Disc, Track};

/// RAII wrapper for libcdio `CdIo_t` handle
struct CdioHandle {
    ptr: *mut CdIo_t,
}

impl CdioHandle {
    fn open(device: Option<&str>) -> Option<Self> {
        let c_device = device.and_then(|d| CString::new(d).ok());
        let device_ptr = c_device.as_ref().map_or(ptr::null(), |c| c.as_ptr());

        let ptr = unsafe { cdio_open(device_ptr, driver_id_t_DRIVER_UNKNOWN) };

        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }
}

impl Drop for CdioHandle {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { cdio_destroy(self.ptr) };
        }
    }
}

/// Read a CD-Text field as a UTF-8 String
fn get_cdtext_field(
    cdio: *mut CdIo_t,
    field: libcdio_sys::cdtext_field_t,
    track: u8,
) -> Option<String> {
    unsafe {
        let cdtext = cdio_get_cdtext(cdio);
        if cdtext.is_null() {
            return None;
        }

        let value = cdtext_get_const(cdtext, field, track);
        if value.is_null() {
            return None;
        }

        CStr::from_ptr(value)
            .to_str()
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

/// Try to read CD-Text from the disc and return a Disc if successful
pub fn read_cdtext() -> Option<Disc> {
    let device = DiscId::default_device();
    debug!("Attempting to read CD-Text from device: {device}");

    let cdio = CdioHandle::open(Some(&device))?;

    // Check if we have CD-Text at all (try to get album title)
    let title = get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_TITLE, 0)?;

    info!("Found CD-Text data, album: {title}");

    let artist = get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_PERFORMER, 0)
        .unwrap_or_else(|| "Unknown".to_string());
    let genre = get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_GENRE, 0);

    let first_track = unsafe { cdio_get_first_track_num(cdio.ptr) };
    let num_tracks = unsafe { cdio_get_num_tracks(cdio.ptr) };

    debug!("CD has {num_tracks} tracks starting at {first_track}");

    let mut tracks = Vec::with_capacity(num_tracks as usize);

    for i in 0..num_tracks {
        let track_num = first_track + i;

        let track_title = get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_TITLE, track_num)
            .unwrap_or_else(|| format!("Track {track_num}"));

        let track_artist =
            get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_PERFORMER, track_num)
                .unwrap_or_else(|| artist.clone());

        let composer =
            get_cdtext_field(cdio.ptr, cdtext_field_t_CDTEXT_FIELD_COMPOSER, track_num);

        tracks.push(Track {
            number: u32::from(track_num),
            title: track_title,
            artist: track_artist,
            duration: 0, // CD-Text doesn't include duration, will be filled later
            composer,
            rip: true,
        });
    }

    Some(Disc {
        title,
        artist,
        year: None, // CD-Text doesn't include year
        genre,
        tracks,
    })
}
