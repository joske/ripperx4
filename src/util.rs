use discid::{DiscError, DiscId};
use log::{debug, warn};

use crate::data::{Config, Disc};

pub(crate) fn read_config() -> Config {
    confy::load("ripperx4", Some("ripperx4")).unwrap_or_default()
}

pub(crate) fn write_config(config: &Config) {
    if let Err(e) = confy::store("ripperx4", Some("ripperx4"), config) {
        warn!("Failed to save config: {e}");
    }
}

pub fn scan_disc() -> Result<DiscId, DiscError> {
    let config: Config = read_config();
    debug!("fake_cdrom={}", config.fake_cdrom);

    DiscId::read(Some(&DiscId::default_device())).or_else(|e| {
        if config.fake_cdrom {
            debug!("CD read failed, using fake disc for testing");
            Ok(fake_discid())
        } else {
            Err(e)
        }
    })
}

#[allow(clippy::cast_sign_loss)]
pub fn lookup_disc(discid: &DiscId) -> Disc {
    let id = discid.id();
    debug!("Looking up disc id={id}");

    match crate::musicbrainz::lookup(&id) {
        Ok(disc) => {
            debug!("Found: {} - {}", disc.artist, disc.title);
            disc
        }
        Err(e) => {
            warn!("MusicBrainz lookup failed: {e}");
            let first = discid.first_track_num() as u32;
            let last = discid.last_track_num() as u32;
            let track_count = last.saturating_sub(first) + 1;
            Disc::with_tracks(track_count)
        }
    }
}

fn fake_discid() -> DiscId {
    // Dire Straits - Money for Nothing (for testing without CD drive)
    let offsets = [
        298_948, 183, 26155, 44233, 64778, 80595, 117_410, 144_120, 159_913, 178_520, 204_803,
        258_763, 277_218,
    ];
    DiscId::put(1, &offsets).expect("hardcoded offsets should be valid")
}

#[cfg(test)]
mod test {
    use super::*;

    fn bad_discid() -> DiscId {
        let offsets = [450, 150, 300];
        DiscId::put(1, &offsets).expect("test offsets should be valid")
    }

    #[test]
    #[ignore]
    fn test_lookup_disc_dire_straits() {
        let disc = lookup_disc(&fake_discid());
        assert_eq!(disc.tracks.len(), 12);
        assert_eq!(disc.title, "Money for Nothing");
    }

    #[test]
    #[ignore]
    fn test_lookup_disc_bad_discid() {
        let disc = lookup_disc(&bad_discid());
        assert_eq!(disc.tracks.len(), 2);
        assert_eq!(disc.title, "Unknown");
        assert_eq!(disc.artist, "Unknown");
        assert_eq!(disc.tracks[0].title, "Unknown");
        assert_eq!(disc.tracks[0].artist, "Unknown");
        assert_eq!(disc.tracks[1].title, "Unknown");
        assert_eq!(disc.tracks[1].artist, "Unknown");
    }
}
