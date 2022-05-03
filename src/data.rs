#[derive(Default, Debug)]
pub struct Data {
    pub disc: Option<Disc>,
}

#[derive(Default, Debug)]
pub struct Disc {
    pub title: String,
    pub artist: String,
    pub year: u16,
    pub tracks: Vec<Track>
}
#[derive(Default, Debug)]
pub struct Track {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub duration: u64,
    pub composer: Option<String>,
}