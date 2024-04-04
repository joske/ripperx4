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
    pub(crate) fn with_tracks(num: i32) -> Disc {
        let mut d = Disc {
            title: "Unknown".to_string(),
            artist: "Unknown".to_string(),
            year: None,
            genre: None,
            tracks: Vec::with_capacity(num as usize),
        };
        for i in 1..=num {
            d.tracks.push(Track {
                number: i as u32,
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
#[derive(Serialize, Deserialize)]
pub enum Encoder {
    MP3,
    OGG,
    FLAC,
    OPUS,
}

#[derive(Serialize, Deserialize)]
pub enum Quality {
    Low,
    Medium,
    High,
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
