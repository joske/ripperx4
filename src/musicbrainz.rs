use crate::data::{Disc, Track};
use anyhow::{Result, anyhow};
use log::debug;
use minidom::Element;
use std::{thread, time::Duration};

const NS: &str = "http://musicbrainz.org/ns/mmd-2.0#";
const USER_AGENT: &str = "ripperX4/0.1.0 (https://github.com/sourcery/ripperx4)";
const RATE_LIMIT_DELAY: Duration = Duration::from_millis(1100); // MusicBrainz requires 1 req/sec

/// Get child element with MusicBrainz namespace
fn get_child<'a>(parent: &'a Element, name: &str) -> Option<&'a Element> {
    parent.get_child(name, NS)
}

/// Lookup a disc by discid on MusicBrainz
/// Returns a `Disc` if found and parsing succeeds
pub fn lookup(discid: &str) -> Result<Disc> {
    let lookup_url = format!("https://musicbrainz.org/ws/2/discid/{discid}");
    debug!("Looking up disc: {lookup_url}");

    let body: String = ureq::get(&lookup_url)
        .header("User-Agent", USER_AGENT)
        .call()?
        .body_mut()
        .read_to_string()?;

    let release_url = get_release_url(&body)?;
    debug!("Found release: {release_url}");

    // Rate limit: MusicBrainz requires max 1 request per second
    thread::sleep(RATE_LIMIT_DELAY);

    let body: String = ureq::get(&release_url)
        .header("User-Agent", USER_AGENT)
        .call()?
        .body_mut()
        .read_to_string()?;

    parse_metadata(&body)
}

/// Extract release URL from disc lookup response
fn get_release_url(body: &str) -> Result<String> {
    let metadata: Element = body.parse()?;
    let disc = metadata
        .children()
        .next()
        .ok_or_else(|| anyhow!("No disc element in response"))?;

    let release_list = get_child(disc, "release-list")
        .ok_or_else(|| anyhow!("No release-list in disc"))?;

    let release = get_child(release_list, "release")
        .ok_or_else(|| anyhow!("No release in release-list"))?;

    let release_id = release
        .attr("id")
        .ok_or_else(|| anyhow!("Release has no id attribute"))?;

    Ok(format!(
        "https://musicbrainz.org/ws/2/release/{release_id}?inc=recordings+artist-credits"
    ))
}

/// Parse release metadata XML into a Disc
fn parse_metadata(xml: &str) -> Result<Disc> {
    let metadata: Element = xml.parse()?;
    let release = metadata
        .children()
        .next()
        .ok_or_else(|| anyhow!("No release element in metadata"))?;

    let mut disc = Disc::default();

    if let Some(title) = get_child(release, "title") {
        disc.title = title.text();
    }

    disc.artist = get_artist(release).unwrap_or_default();

    let medium_list = get_child(release, "medium-list")
        .ok_or_else(|| anyhow!("No medium-list in release"))?;

    let medium = medium_list
        .children()
        .next()
        .ok_or_else(|| anyhow!("No medium in medium-list"))?;

    let track_list = get_child(medium, "track-list")
        .ok_or_else(|| anyhow!("No track-list in medium"))?;

    for (i, track) in track_list.children().enumerate() {
        let mut dtrack = Track::default();

        // Track numbers are 1-based, use index+1 as fallback
        dtrack.number = get_child(track, "number")
            .and_then(|n| n.text().parse().ok())
            .unwrap_or((i + 1) as u32);

        if let Some(recording) = get_child(track, "recording") {
            if let Some(title) = get_child(recording, "title") {
                dtrack.title = title.text();
            }
            dtrack.artist = get_artist(recording).unwrap_or_default();
        }

        dtrack.rip = true;
        disc.tracks.push(dtrack);
    }

    Ok(disc)
}

/// Extract artist name from an element with artist-credit child
fn get_artist(element: &Element) -> Result<String> {
    let artist_credit = get_child(element, "artist-credit")
        .ok_or_else(|| anyhow!("No artist-credit element"))?;

    let name_credit = get_child(artist_credit, "name-credit")
        .ok_or_else(|| anyhow!("No name-credit in artist-credit"))?;

    let artist = get_child(name_credit, "artist")
        .ok_or_else(|| anyhow!("No artist in name-credit"))?;

    let name = get_child(artist, "name")
        .ok_or_else(|| anyhow!("No name in artist"))?;

    Ok(name.text())
}

#[cfg(test)]
mod test {
    use std::{env, fs};

    use super::{get_release_url, lookup, parse_metadata};
    use anyhow::Result;

    #[test]
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
    fn test_parse_metadata_good() -> Result<()> {
        let mut path = env::var("CARGO_MANIFEST_DIR")?;
        path.push_str("/resources/test/direstraits-releases-metadata.xml");
        let contents = fs::read_to_string(path)?;
        let disc = parse_metadata(&contents)?;
        assert_eq!("Dire Straits", disc.artist);
        assert_eq!("Money for Nothing", disc.title);
        assert_eq!(12, disc.tracks.len());
        assert_eq!("Sultans of Swing", disc.tracks[0].title);
        assert_eq!("Dire Straits", disc.tracks[0].artist);
        assert_eq!(1, disc.tracks[0].number);
        Ok(())
    }

    #[test]
    fn parse_metadata_bad_non_xml() {
        let result = parse_metadata("not xml");
        assert!(result.is_err());
    }

    #[test]
    fn parse_metadata_bad_xml_empty() {
        let result = parse_metadata(r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#);
        assert!(result.is_err());
    }

    #[test]
    fn get_release_url_bad_non_xml() {
        let result = get_release_url("not xml");
        assert!(result.is_err());
    }

    #[test]
    fn get_release_url_bad_xml_empty() {
        let result = get_release_url(r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_bad_discid() {
        let result = lookup("invalid-disc-id");
        assert!(result.is_err());
    }
}
