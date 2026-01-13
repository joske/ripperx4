use serde::{Deserialize, Serialize};

#[derive(Default, Debug)]
pub struct Disc {
    pub title: String,
    pub artist: String,
    pub year: Option<u16>,
    pub genre: Option<String>,
    pub tracks: Vec<Track>,
}

impl Disc {
    pub(crate) fn with_tracks(num: u32) -> Disc {
        let mut d = Disc {
            title: "Unknown".to_string(),
            artist: "Unknown".to_string(),
            year: None,
            genre: None,
            tracks: Vec::new(),
        };
        for i in 1..=num {
            d.tracks.push(Track {
                number: i,
                title: "Unknown".to_string(),
                artist: "Unknown".to_string(),
                duration: 0,
                composer: None,
                rip: true,
            });
        }
        d
    }
}

#[derive(Default, Debug)]
pub struct Track {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub duration: u64,
    pub composer: Option<String>,
    pub rip: bool,
}

#[derive(Default, Debug)]
pub struct Data {
    pub disc: Option<Disc>,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Clone, Copy)]
pub enum Encoder {
    #[default]
    MP3,
    OGG,
    FLAC,
    OPUS,
    WAV,
    AAC,
}

impl Encoder {
    pub const OPTIONS: &[&str] = &["mp3", "ogg", "flac", "opus", "wav", "aac"];

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::MP3,
            1 => Self::OGG,
            2 => Self::FLAC,
            3 => Self::OPUS,
            4 => Self::WAV,
            5 => Self::AAC,
            _ => Self::default(),
        }
    }

    pub fn to_index(self) -> u32 {
        match self {
            Self::MP3 => 0,
            Self::OGG => 1,
            Self::FLAC => 2,
            Self::OPUS => 3,
            Self::WAV => 4,
            Self::AAC => 5,
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::MP3 => ".mp3",
            Self::FLAC => ".flac",
            Self::OGG | Self::OPUS => ".ogg",
            Self::WAV => ".wav",
            Self::AAC => ".m4a",
        }
    }

    pub fn has_quality_setting(self) -> bool {
        !matches!(self, Self::WAV)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Clone, Copy)]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

impl Quality {
    pub const OPTIONS: &[&str] = &["low", "medium", "high"];

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::Low,
            1 => Self::Medium,
            2 => Self::High,
            _ => Self::default(),
        }
    }

    pub fn to_index(self) -> u32 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }

    /// LAME MP3 encoder quality (0=best, 9=worst) - VBR mode
    pub fn mp3_quality(self) -> f32 {
        match self {
            Self::Low => 9.0,
            Self::Medium => 5.0,
            Self::High => 0.0,
        }
    }

    /// Vorbis encoder quality (0.0-1.0)
    pub fn vorbis_quality(self) -> f32 {
        match self {
            Self::Low => 0.2,
            Self::Medium => 0.5,
            Self::High => 0.9,
        }
    }

    /// FLAC compression level (0-8, higher = more compression)
    pub fn flac_level(self) -> &'static str {
        match self {
            Self::Low => "2",
            Self::Medium => "5",
            Self::High => "8",
        }
    }

    /// Opus bitrate in bits/second
    pub fn opus_bitrate(self) -> i32 {
        match self {
            Self::Low => 64_000,
            Self::Medium => 128_000,
            Self::High => 256_000,
        }
    }

    /// AAC bitrate in bits/second
    pub fn aac_bitrate(self) -> i32 {
        match self {
            Self::Low => 128_000,
            Self::Medium => 192_000,
            Self::High => 256_000,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Clone, Copy)]
pub enum FilePattern {
    #[default]
    ArtistAlbum,
    ArtistDashAlbum,
    AlbumOnly,
    Custom,
}

impl FilePattern {
    pub const OPTIONS: &[&str] = &["Artist/Album", "Artist - Album", "Album only", "Custom"];

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::ArtistAlbum,
            1 => Self::ArtistDashAlbum,
            2 => Self::AlbumOnly,
            3 => Self::Custom,
            _ => Self::default(),
        }
    }

    pub fn to_index(self) -> u32 {
        match self {
            Self::ArtistAlbum => 0,
            Self::ArtistDashAlbum => 1,
            Self::AlbumOnly => 2,
            Self::Custom => 3,
        }
    }

    /// Returns the pattern template string for this preset
    pub fn template(self, custom: &str) -> &str {
        match self {
            Self::ArtistAlbum => "{artist}/{album}/{number} - {title}",
            Self::ArtistDashAlbum => "{artist} - {album}/{number} - {title}",
            Self::AlbumOnly => "{album}/{number} - {title}",
            Self::Custom => custom,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    pub encode_path: String,
    pub encoder: Encoder,
    pub quality: Quality,
    pub fake_cdrom: bool,
    #[serde(default)]
    pub eject_when_done: bool,
    #[serde(default)]
    pub create_playlist: bool,
    #[serde(default)]
    pub file_pattern: FilePattern,
    #[serde(default)]
    pub custom_pattern: String,
    #[serde(default)]
    pub open_folder_when_done: bool,
    /// Enable paranoia error correction for scratched discs (slower but more reliable)
    #[serde(default)]
    pub use_paranoia: bool,
}

impl Default for Config {
    fn default() -> Self {
        let home = home::home_dir().expect("Failed to get home dir!");
        let path = format!("{}/Music/", home.display());
        Config {
            encode_path: path,
            encoder: Encoder::MP3,
            quality: Quality::Medium,
            fake_cdrom: false,
            eject_when_done: false,
            create_playlist: false,
            file_pattern: FilePattern::default(),
            custom_pattern: String::new(),
            open_folder_when_done: false,
            use_paranoia: true,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn disc_with_tracks_initializes_defaults() {
        let disc = Disc::with_tracks(3);
        assert_eq!(disc.title, "Unknown");
        assert_eq!(disc.artist, "Unknown");
        assert_eq!(disc.tracks.len(), 3);

        for (idx, track) in disc.tracks.iter().enumerate() {
            let number = u32::try_from(idx + 1).unwrap();
            assert_eq!(track.number, number);
            assert_eq!(track.title, "Unknown");
            assert_eq!(track.artist, "Unknown");
            assert_eq!(track.duration, 0);
            assert!(track.composer.is_none());
            assert!(track.rip);
        }
    }

    #[test]
    fn encoder_index_roundtrip_and_extensions() {
        let cases = [
            (Encoder::MP3, 0, ".mp3"),
            (Encoder::OGG, 1, ".ogg"),
            (Encoder::FLAC, 2, ".flac"),
            (Encoder::OPUS, 3, ".ogg"),
            (Encoder::WAV, 4, ".wav"),
            (Encoder::AAC, 5, ".m4a"),
        ];
        for (encoder, idx, ext) in cases {
            assert_eq!(Encoder::from_index(idx), encoder);
            assert_eq!(encoder.to_index(), idx);
            assert_eq!(encoder.file_extension(), ext);
        }

        assert_eq!(Encoder::from_index(99), Encoder::default());
    }

    #[test]
    fn quality_index_roundtrip_and_settings() {
        let cases = [
            (Quality::Low, 0, 9.0, 0.2, "2", 64_000, 128_000),
            (Quality::Medium, 1, 5.0, 0.5, "5", 128_000, 192_000),
            (Quality::High, 2, 0.0, 0.9, "8", 256_000, 256_000),
        ];
        #[allow(clippy::float_cmp)]
        for (quality, idx, mp3_q, vorbis_q, flac_lvl, opus_br, aac_br) in cases {
            assert_eq!(Quality::from_index(idx), quality);
            assert_eq!(quality.to_index(), idx);
            assert_eq!(quality.mp3_quality(), mp3_q);
            assert_eq!(quality.vorbis_quality(), vorbis_q);
            assert_eq!(quality.flac_level(), flac_lvl);
            assert_eq!(quality.opus_bitrate(), opus_br);
            assert_eq!(quality.aac_bitrate(), aac_br);
        }

        assert_eq!(Quality::from_index(99), Quality::default());
    }

    #[test]
    fn config_default_uses_home_music_path() {
        let home = home::home_dir().expect("Failed to get home dir!");
        let expected = format!("{}/Music/", home.display());
        let config = Config::default();
        assert_eq!(config.encode_path, expected);
        assert_eq!(config.encoder, Encoder::MP3);
        assert_eq!(config.quality, Quality::Medium);
        assert!(!config.fake_cdrom);
        assert!(!config.eject_when_done);
        assert!(!config.create_playlist);
        assert_eq!(config.file_pattern, FilePattern::ArtistAlbum);
        assert!(config.custom_pattern.is_empty());
        assert!(!config.open_folder_when_done);
        assert!(config.use_paranoia);
    }

    // ==================== Edge case tests ====================

    #[test]
    fn disc_with_zero_tracks() {
        let disc = Disc::with_tracks(0);
        assert!(disc.tracks.is_empty());
        assert_eq!(disc.title, "Unknown");
        assert_eq!(disc.artist, "Unknown");
    }

    #[test]
    fn disc_default_has_no_tracks() {
        let disc = Disc::default();
        assert!(disc.tracks.is_empty());
        assert!(disc.title.is_empty());
        assert!(disc.artist.is_empty());
        assert!(disc.year.is_none());
        assert!(disc.genre.is_none());
    }

    #[test]
    fn track_default_values() {
        let track = Track::default();
        assert_eq!(track.number, 0);
        assert!(track.title.is_empty());
        assert!(track.artist.is_empty());
        assert_eq!(track.duration, 0);
        assert!(track.composer.is_none());
        assert!(!track.rip);
    }

    #[test]
    fn encoder_all_variants_have_non_empty_extensions() {
        let encoders = [
            Encoder::MP3,
            Encoder::OGG,
            Encoder::FLAC,
            Encoder::OPUS,
            Encoder::WAV,
            Encoder::AAC,
        ];
        for encoder in encoders {
            let ext = encoder.file_extension();
            assert!(
                !ext.is_empty(),
                "Extension for {encoder:?} should not be empty"
            );
            assert!(
                ext.starts_with('.'),
                "Extension for {encoder:?} should start with '.'"
            );
        }
    }

    #[test]
    fn encoder_options_matches_variant_count() {
        // Ensure OPTIONS array stays in sync with enum variants
        assert_eq!(Encoder::OPTIONS.len(), 6);
        assert_eq!(Encoder::OPTIONS[0], "mp3");
        assert_eq!(Encoder::OPTIONS[1], "ogg");
        assert_eq!(Encoder::OPTIONS[2], "flac");
        assert_eq!(Encoder::OPTIONS[3], "opus");
        assert_eq!(Encoder::OPTIONS[4], "wav");
        assert_eq!(Encoder::OPTIONS[5], "aac");
    }

    #[test]
    fn quality_options_matches_variant_count() {
        assert_eq!(Quality::OPTIONS.len(), 3);
        assert_eq!(Quality::OPTIONS[0], "low");
        assert_eq!(Quality::OPTIONS[1], "medium");
        assert_eq!(Quality::OPTIONS[2], "high");
    }

    #[test]
    fn quality_mp3_values_in_valid_range() {
        // LAME quality is 0-9 where 0=best, 9=worst
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let val = quality.mp3_quality();
            assert!(
                (0.0..=9.0).contains(&val),
                "MP3 quality {val} out of range for {quality:?}"
            );
        }
    }

    #[test]
    fn quality_vorbis_values_in_valid_range() {
        // Vorbis quality is 0.0-1.0
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let val = quality.vorbis_quality();
            assert!(
                (0.0..=1.0).contains(&val),
                "Vorbis quality {val} out of range for {quality:?}"
            );
        }
    }

    #[test]
    fn quality_flac_levels_are_valid() {
        // FLAC compression level is 0-8
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let level: u32 = quality
                .flac_level()
                .parse()
                .expect("FLAC level should be numeric");
            assert!(
                (0..=8).contains(&level),
                "FLAC level {level} out of range for {quality:?}"
            );
        }
    }

    #[test]
    fn quality_opus_bitrates_are_reasonable() {
        // Opus bitrate should be positive and reasonable (32k-512k typical range)
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let bitrate = quality.opus_bitrate();
            assert!(
                bitrate > 0,
                "Opus bitrate should be positive for {quality:?}"
            );
            assert!(
                bitrate >= 32_000,
                "Opus bitrate {bitrate} too low for {quality:?}"
            );
            assert!(
                bitrate <= 512_000,
                "Opus bitrate {bitrate} too high for {quality:?}"
            );
        }
    }

    #[test]
    fn quality_aac_bitrates_are_reasonable() {
        // AAC bitrate should be positive and reasonable (64k-320k typical range)
        for quality in [Quality::Low, Quality::Medium, Quality::High] {
            let bitrate = quality.aac_bitrate();
            assert!(
                bitrate > 0,
                "AAC bitrate should be positive for {quality:?}"
            );
            assert!(
                bitrate >= 64_000,
                "AAC bitrate {bitrate} too low for {quality:?}"
            );
            assert!(
                bitrate <= 320_000,
                "AAC bitrate {bitrate} too high for {quality:?}"
            );
        }
    }

    #[test]
    fn data_default_has_no_disc() {
        let data = Data::default();
        assert!(data.disc.is_none());
    }

    // ==================== FilePattern tests ====================

    #[test]
    fn file_pattern_index_roundtrip() {
        let cases = [
            (FilePattern::ArtistAlbum, 0),
            (FilePattern::ArtistDashAlbum, 1),
            (FilePattern::AlbumOnly, 2),
            (FilePattern::Custom, 3),
        ];
        for (pattern, idx) in cases {
            assert_eq!(FilePattern::from_index(idx), pattern);
            assert_eq!(pattern.to_index(), idx);
        }
        assert_eq!(FilePattern::from_index(99), FilePattern::default());
    }

    #[test]
    fn file_pattern_options_matches_variant_count() {
        assert_eq!(FilePattern::OPTIONS.len(), 4);
    }

    #[test]
    fn file_pattern_templates_contain_placeholders() {
        let custom = "{custom}";
        assert!(
            FilePattern::ArtistAlbum
                .template(custom)
                .contains("{artist}")
        );
        assert!(
            FilePattern::ArtistAlbum
                .template(custom)
                .contains("{album}")
        );
        assert!(
            FilePattern::ArtistDashAlbum
                .template(custom)
                .contains("{artist}")
        );
        assert!(FilePattern::AlbumOnly.template(custom).contains("{album}"));
        assert!(!FilePattern::AlbumOnly.template(custom).contains("{artist}"));
        assert_eq!(FilePattern::Custom.template(custom), custom);
    }
}
