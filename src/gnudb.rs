use anyhow::Result;
use discid::DiscId;
use log::debug;

use crate::data::{Disc, Track};

pub(crate) fn lookup(discid: &DiscId) -> Result<Disc> {
    let matches = gnudb::http_query("gnudb.gnudb.org", 80, discid)?;
    let first = matches
        .first()
        .ok_or_else(|| anyhow::anyhow!("No GNUDB matches found"))?;
    let disc = gnudb::http_read("gnudb.gnudb.org", 80, first)?;
    debug!("Found GNUDB data: {} - {}", disc.artist, disc.title);
    Ok(Disc::from(disc))
}

impl From<gnudb::Track> for Track {
    fn from(t: gnudb::Track) -> Self {
        Track {
            number: t.number,
            title: t.title,
            artist: t.artist,
            duration: t.duration,
            composer: t.composer,
            rip: true,
        }
    }
}

impl From<gnudb::Disc> for Disc {
    fn from(d: gnudb::Disc) -> Self {
        let tracks = d.tracks.into_iter().map(Track::from).collect();
        Disc {
            title: d.title,
            artist: d.artist,
            year: d.year,
            genre: d.genre,
            tracks,
        }
    }
}
