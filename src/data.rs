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
