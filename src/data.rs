#[derive(Default)]
pub struct Disc {
    pub title: String,
    pub artist: String,
    pub tracks: Vec<Track>
}

pub struct Track {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub duration: u64,
    pub composer: Option<String>,
}