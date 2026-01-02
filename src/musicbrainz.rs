use crate::data::{Disc, Track};
use anyhow::{Result, anyhow};
use log::debug;
use musicbrainz_rs::{
    Fetch,
    entity::{discid::Discid as MBDiscid, release::Release},
};

/// Lookup a disc by discid on `MusicBrainz`
/// Returns a `Disc` if found and parsing succeeds
pub fn lookup(discid: &str) -> Result<Disc> {
    debug!("Looking up disc id={discid}");

    let result: MBDiscid = MBDiscid::fetch()
        .id(discid)
        .with_recordings()
        .with_artist_credits()
        .execute()?;

    let releases: &Vec<Release> = result
        .releases
        .as_ref()
        .ok_or_else(|| anyhow!("No releases in response"))?;

    debug!("Found {} releases", releases.len());

    let release = releases
        .first()
        .ok_or_else(|| anyhow!("No releases found for disc"))?;

    let artist = release
        .artist_credit
        .as_ref()
        .and_then(|c| c.first())
        .map(|c| c.artist.name.clone())
        .unwrap_or_default();

    let tracks = extract_tracks(release);

    let disc = Disc {
        title: release.title.clone(),
        artist,
        tracks,
        ..Default::default()
    };

    debug!(
        "Parsed: {} - {} ({} tracks)",
        disc.artist,
        disc.title,
        disc.tracks.len()
    );
    Ok(disc)
}

fn extract_tracks(release: &Release) -> Vec<Track> {
    let Some(media) = &release.media else {
        return Vec::new();
    };
    let Some(medium) = media.first() else {
        return Vec::new();
    };
    let Some(tracks) = &medium.tracks else {
        return Vec::new();
    };

    tracks
        .iter()
        .map(|track| {
            let artist = track
                .artist_credit
                .as_ref()
                .and_then(|c| c.first())
                .map(|c| c.artist.name.clone())
                .unwrap_or_default();

            let duration = track.length.map_or(0, |l| u64::from(l / 1000));

            Track {
                number: track.position,
                title: track.title.clone(),
                artist,
                duration,
                rip: true,
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::lookup;
    use anyhow::Result;

    #[test]
    #[ignore = "these tests require network access to MusicBrainz, so ignore them by default"]
    fn test_good_net() -> Result<()> {
        let disc = lookup("xA3p59dQpJpDXZYHz1SSQ491oaU-")?;
        assert_eq!("Dire Straits", disc.artist);
        assert_eq!("Money for Nothing", disc.title);
        assert_eq!(12, disc.tracks.len());
        assert_eq!("Sultans of Swing", disc.tracks[0].title);
        assert_eq!("Dire Straits", disc.tracks[0].artist);
        assert_eq!(1, disc.tracks[0].number);
        Ok(())
    }

    #[test]
    #[ignore = "these tests require network access to MusicBrainz, so ignore them by default"]
    fn test_bad_discid() {
        let result = lookup("invalid-disc-id");
        assert!(result.is_err());
    }
}
