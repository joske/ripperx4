use serde::{Deserialize, Serialize};

#[derive(Default, Debug)]
pub struct Disc {
    pub title: String,
    pub artist: String,
    pub year: Option<u16>,
    pub genre: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Default, Debug)]
pub struct Track {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub duration: u64,
    pub composer: Option<String>,
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
}

impl Default for Config {
    fn default() -> Self {
        let home = home::home_dir().expect("Failed to get home dir!");
        let path = format!("{}/Music/", home.display());
        Config {
            encode_path: path,
            encoder: Encoder::MP3,
            quality: Quality::Medium,
        }
    }
}
