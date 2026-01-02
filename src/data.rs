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
                rip: false,
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
#[derive(Serialize, Deserialize, Default, Clone, Copy)]
pub enum Encoder {
    #[default]
    MP3,
    OGG,
    FLAC,
    OPUS,
}

impl Encoder {
    pub const OPTIONS: &[&str] = &["mp3", "ogg", "flac", "opus"];

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::MP3,
            1 => Self::OGG,
            2 => Self::FLAC,
            3 => Self::OPUS,
            _ => Self::default(),
        }
    }

    pub fn to_index(self) -> u32 {
        match self {
            Self::MP3 => 0,
            Self::OGG => 1,
            Self::FLAC => 2,
            Self::OPUS => 3,
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::MP3 => ".mp3",
            Self::FLAC => ".flac",
            Self::OGG | Self::OPUS => ".ogg",
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Copy)]
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

    /// LAME MP3 encoder quality (0=best, 9=worst)
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
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    pub encode_path: String,
    pub encoder: Encoder,
    pub quality: Quality,
    pub fake_cdrom: bool,
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
        }
    }
}
