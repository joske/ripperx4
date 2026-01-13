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

    // Try CD-Text first (local, instant, no network required)
    if let Some(disc) = crate::cdtext::read_cdtext() {
        debug!("Found CD-Text: {} - {}", disc.artist, disc.title);
        return disc;
    }
    debug!("No CD-Text found, trying network lookup");

    match crate::musicbrainz::lookup(&id) {
        Ok(disc) => {
            debug!("Found: {} - {}", disc.artist, disc.title);
            disc
        }
        Err(e) => {
            warn!("MusicBrainz lookup failed: {e}");
            crate::gnudb::lookup(discid).unwrap_or_else(|e| {
                warn!("GNUDB lookup failed: {e}");
                let first = discid.first_track_num();
                let last = discid.last_track_num();
                let track_count = last.saturating_sub(first) + 1;
                Disc::with_tracks(track_count.cast_unsigned())
            })
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
    use crate::data::{Encoder, FilePattern, Quality};

    fn bad_discid() -> DiscId {
        let offsets = [450, 150, 300];
        DiscId::put(1, &offsets).expect("test offsets should be valid")
    }

    #[test]
    #[ignore = "these tests require network access to MusicBrainz, so ignore them by default"]
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

    // ==================== Config tests ====================

    #[test]
    #[ignore = "modifies real config file - run manually"]
    fn config_roundtrip_preserves_values() {
        // Save original config to restore later
        let original = read_config();

        // Create test config with non-default values
        let test_config = Config {
            encode_path: "/tmp/test_music/".to_string(),
            encoder: Encoder::FLAC,
            quality: Quality::High,
            fake_cdrom: true,
            eject_when_done: true,
            create_playlist: true,
            file_pattern: FilePattern::ArtistDashAlbum,
            custom_pattern: String::new(),
            open_folder_when_done: true,
        };

        // Write and read back
        write_config(&test_config);
        let loaded = read_config();

        // Verify values match
        assert_eq!(loaded.encode_path, test_config.encode_path);
        assert_eq!(loaded.encoder, test_config.encoder);
        assert_eq!(loaded.quality, test_config.quality);
        assert_eq!(loaded.fake_cdrom, test_config.fake_cdrom);
        assert_eq!(loaded.eject_when_done, test_config.eject_when_done);
        assert_eq!(loaded.create_playlist, test_config.create_playlist);

        // Restore original config
        write_config(&original);
    }

    #[test]
    fn read_config_returns_valid_config() {
        // This tests that read_config doesn't panic and returns a usable config
        let config = read_config();
        // encode_path should be a non-empty string
        assert!(!config.encode_path.is_empty());
    }

    #[test]
    fn fake_discid_is_valid() {
        let disc = fake_discid();
        // Dire Straits - Money for Nothing has 12 tracks
        assert_eq!(disc.last_track_num() - disc.first_track_num() + 1, 12);
        // The disc ID should be consistent
        assert!(!disc.id().is_empty());
    }
}
