use discid::{DiscError, DiscId};
use log::debug;

use crate::data::{Config, Disc};

pub fn scan_disc() -> Result<DiscId, DiscError> {
    let config: Config = confy::load("ripperx4", None).expect("failed to load config");
    debug!("fake={}", config.fake_cdrom);
    match DiscId::read(Some(&DiscId::default_device())) {
        Ok(discid) => Ok(discid),
        Err(e) => {
            if config.fake_cdrom {
                debug!("fake_cdrom is set, using hardcoded offsets");
                // for testing on machine without CDROM drive: hardcode offsets of a dire straits disc
                Ok(fake_discid())
            } else {
                Err(e)
            }
        }
    }
}

#[allow(clippy::cast_sign_loss)]
pub fn lookup_disc(discid: &DiscId) -> Disc {
    debug!("id={}", discid.id());
    if let Ok(disc) = crate::musicbrainz::lookup(&discid.id()) {
        disc
    } else {
        let last = discid.last_track_num() as u32;
        let first = discid.first_track_num() as u32;
        let num: u32 = last.saturating_sub(first) + 1;
        Disc::with_tracks(num)
    }
}

fn fake_discid() -> DiscId {
    let offsets = [
        298_948, 183, 26155, 44233, 64778, 80595, 117_410, 144_120, 159_913, 178_520, 204_803,
        258_763, 277_218,
    ];
    DiscId::put(1, &offsets).unwrap() // this is for testing only so this unwrap is ok
}

#[cfg(test)]
mod test {
    use super::*;

    fn bad_discid() -> DiscId {
        let offsets = [450, 150, 300];
        DiscId::put(1, &offsets).unwrap() // this is for testing only so this unwrap is ok
    }

    #[test]
    fn test_lookup_disc_dire_straits() {
        let disc = lookup_disc(&fake_discid());
        assert_eq!(disc.tracks.len(), 12);
        assert_eq!(disc.title, "Money for Nothing");
    }

    #[test]
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
