use crate::data::{Disc, Track};
use anyhow::{anyhow, Result};
use minidom::Element;

macro_rules! get_child {
    ($parent:ident, $child:literal) => {
        $parent.get_child($child, "http://musicbrainz.org/ns/mmd-2.0#")
    };
    ($parent:ident, $child:literal, $err:literal) => {
        get_child!($parent, $child).ok_or(anyhow!($err))
    };
}

macro_rules! get_first_child {
    ($parent:ident) => {
        $parent.children().next()
    };
    ($parent:ident, $err:literal) => {
        get_first_child!($parent).ok_or(anyhow!($err))
    };
}

/// Lookup a disc by discid on musicbrainz
/// Returns a `Disc` if a disc was found and parsing metadata succeeds
pub fn lookup(discid: &str) -> Result<Disc> {
    let lookup = format!("https://musicbrainz.org/ws/2/discid/{discid}");
    let body: String = ureq::get(&lookup).call()?.into_string()?;
    let release = get_release_url(&body)?;
    let body: String = ureq::get(&release).call()?.into_string()?;
    parse_metadata(&body)
}

/// Return an URL to a release for the given disc
/// Parses the XML returned by the query on discid
fn get_release_url(body: &str) -> Result<String> {
    let metadata: Element = body.parse()?;
    let disc = get_first_child!(metadata, "failed to get disc")?;
    let release_list = get_child!(disc, "release-list", "failed to get release list")?;
    let release = get_child!(release_list, "release", "failed to get release")?;
    let release_id = release
        .attr("id")
        .ok_or(anyhow!("failed to get release id"))?;
    Ok(format!(
        "https://musicbrainz.org/ws/2/release/{release_id}?inc=%20recordings+artist-credits"
    ))
}

/// Parse the metadata for the given release
/// Returns a `Disc` if parsing succeeds
fn parse_metadata(xml: &str) -> Result<Disc> {
    let metadata: Element = xml.parse()?;
    let release = get_first_child!(metadata, "failed to get release")?;
    let mut disc = Disc::default();
    if let Some(title) = get_child!(release, "title") {
        disc.title = title.text();
    }

    disc.artist = get_artist(release)?;

    let medium_list = get_child!(release, "medium-list", "failed to get medium list")?;
    let medium = get_first_child!(medium_list, "failed to get medium")?;
    let track_list = get_child!(medium, "track-list", "failed to get track list")?;
    for (i, track) in track_list.children().enumerate() {
        let mut dtrack = Track::default();
        let num: Option<u32> = get_child!(track, "number").and_then(|num| num.text().parse().ok());
        dtrack.number = num.unwrap_or(u32::try_from(i)?);

        if let Some(recording) = get_child!(track, "recording") {
            if let Some(title) = get_child!(recording, "title") {
                dtrack.title = title.text();
            }
            dtrack.artist = get_artist(recording).unwrap_or_default();
        }
        disc.tracks.push(dtrack);
    }
    Ok(disc)
}

/// Parse out the Artist name from a `artist-credit` XML element
fn get_artist(element: &Element) -> Result<String> {
    let artist_credit = get_child!(element, "artist-credit", "failed to get artist credit")?;
    let name_credit = get_child!(artist_credit, "name-credit", "failed to get name credit")?;
    let artist = get_child!(name_credit, "artist", "failed to get artist")?;
    Ok(get_child!(artist, "name", "failed to get artist name")?.text())
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
    fn parse_metadata_bad_non_xml() -> Result<()> {
        let e = parse_metadata("brol");
        assert!(e.is_err());
        Ok(())
    }

    #[test]
    fn parse_metadata_bad_xml_no_releases() -> Result<()> {
        let e =
            parse_metadata(r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#);
        assert!(e.is_err());
        Ok(())
    }

    #[test]
    fn parse_disc_bad_non_xml() -> Result<()> {
        let e = get_release_url("brol");
        assert!(e.is_err());
        Ok(())
    }

    #[test]
    fn parse_disc_bad_xml_no_discs() -> Result<()> {
        let e =
            get_release_url(r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#);
        assert!(e.is_err());
        Ok(())
    }

    #[test]
    fn parse_disc_bad_xml_discs() -> Result<()> {
        let e =
            get_release_url(r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#);
        assert!(e.is_err());
        Ok(())
    }

    #[test]
    fn test_bad_discid() -> Result<()> {
        let disc = lookup("dees besta zeker ni");
        assert!(disc.is_err());
        Ok(())
    }
}
