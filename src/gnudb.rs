use discid::DiscId;
use log::debug;

use crate::data::{Disc, Track};

pub(crate) fn lookup(discid: &DiscId) -> Disc {
    let first = discid.first_track_num();
    let last = discid.last_track_num();
    let track_count = last.saturating_sub(first) + 1;
    let empty = Disc::with_tracks(track_count.cast_unsigned());
    let disc = gnudb::http_query("gnudb.gnudb.org", 80, discid).unwrap_or_default();
    if disc.is_empty() {
        debug!("No GNUDB data found, returning empty disc");
        empty
    } else {
        let disc = disc.first().unwrap();
        let disc = gnudb::http_read("gnudb.gnudb.org", 80, disc).unwrap_or_default();
        debug!("Found GNUDB data: {} - {}", disc.artist, disc.title);
        Disc::from(disc)
    }
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
