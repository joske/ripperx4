use std::error::Error;

use minidom::Element;

use crate::data::{Disc, Track};

macro_rules! get_child {
    ($parent:ident, $child:expr) => {
        $parent.get_child($child, "http://musicbrainz.org/ns/mmd-2.0#")
    };
}

pub fn lookup(discid: &str) -> Result<Disc, Box<dyn Error>> {
    let lookup = format!("https://musicbrainz.org/ws/2/discid/{}", discid);
    let body: String = ureq::get(lookup.as_str()).call()?.into_string()?;
    let release = parse_disc(body)?;
    let body: String = ureq::get(release.as_str()).call()?.into_string()?;
    parse_metadata(body)
}

fn parse_disc(body: String) -> Result<String, Box<dyn Error>> {
    let metadata: minidom::Element = body.parse()?;
    if let Some(disc) = metadata.children().next() {
        if let Some(release_list) = get_child!(disc, "release-list") {
            if let Some(release) = get_child!(release_list, "release") {
                if let Some(release_id) = release.attr("id") {
                    let release = format!(
                        "https://musicbrainz.org/ws/2/release/{}?inc=%20recordings+artist-credits",
                        release_id
                    );
                    return Ok(release);
                }
            }
        }
    }
    Err("Failed to parse disc".into())
}

fn parse_metadata(xml: String) -> Result<Disc, Box<dyn Error>> {
    let metadata: minidom::Element = xml.parse()?;
    let release = metadata.children().next();
    if let Some(release) = release {
        let mut disc = Disc {
            ..Default::default()
        };
        if let Some(title) = get_child!(release, "title") {
            disc.title = title.text();
        }

        disc.artist = get_artist(release).unwrap_or_else(|| "".to_owned());

        if let Some(medium_list) = get_child!(release, "medium-list") {
            if let Some(medium) = medium_list.children().next() {
                if let Some(track_list) = get_child!(medium, "track-list") {
                    for track in track_list.children() {
                        let mut dtrack = Track {
                            ..Default::default()
                        };
                        if let Some(num) = get_child!(track, "number") {
                            dtrack.number = num.text().parse().unwrap_or(0);
                        }

                        if let Some(recording) = get_child!(track, "recording") {
                            if let Some(title) = get_child!(recording, "title") {
                                dtrack.title = title.text();
                            }
                            dtrack.artist = get_artist(recording).unwrap_or_else(|| "".to_owned());
                        }
                        disc.tracks.push(dtrack);
                    }
                }
            }
            return Ok(disc);
        }
    }
    Err("Failed to parse metadata".into())
}

fn get_artist(element: &Element) -> Option<String> {
    let artist_credit = get_child!(element, "artist-credit")?;
    let name_credit = get_child!(artist_credit, "name-credit")?;
    let artist = get_child!(name_credit, "artist")?;
    Some(get_child!(artist, "name")?.text())
}

#[cfg(test)]
mod test {
    use std::{env, fs};

    use super::{lookup, parse_disc, parse_metadata};

    #[test]
    fn test_good_net() {
        let disc = lookup("xA3p59dQpJpDXZYHz1SSQ491oaU-");
        assert!(disc.is_ok());
        let disc = disc.unwrap();
        assert_eq!("Dire Straits", disc.artist);
        assert_eq!("Money for Nothing", disc.title);
        assert_eq!(12, disc.tracks.len());
        assert_eq!("Sultans of Swing", disc.tracks[0].title);
        assert_eq!("Dire Straits", disc.tracks[0].artist);
    }

    #[test]
    fn test_parse_metadata_good() {
        let mut path = env::var("CARGO_MANIFEST_DIR").unwrap();
        path.push_str("/resources/test/direstraits-releases-metadata.xml");
        let contents = fs::read_to_string(path).unwrap();
        let disc = parse_metadata(contents);
        assert!(disc.is_ok());
        let disc = disc.unwrap();
        assert_eq!("Dire Straits", disc.artist);
        assert_eq!("Money for Nothing", disc.title);
        assert_eq!(12, disc.tracks.len());
        assert_eq!("Sultans of Swing", disc.tracks[0].title);
        assert_eq!("Dire Straits", disc.tracks[0].artist);
    }

    #[test]
    fn parse_metadata_bad_non_xml() {
        let e = parse_metadata("brol".into());
        assert!(e.is_err());
    }

    #[test]
    fn parse_metadata_bad_xml_no_releases() {
        let e = parse_metadata(
            r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#.into(),
        );
        assert!(e.is_err());
    }

    #[test]
    fn parse_disc_bad_non_xml() {
        let e = parse_disc("brol".into());
        assert!(e.is_err());
    }

    #[test]
    fn parse_disc_bad_xml_no_discs() {
        let e = parse_disc(
            r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#.into(),
        );
        assert!(e.is_err());
    }

    #[test]
    fn parse_disc_bad_xml_discs() {
        let e = parse_disc(
            r#"<metadata xmlns="http://musicbrainz.org/ns/mmd-2.0#"></metadata>"#.into(),
        );
        assert!(e.is_err());
    }

    #[test]
    fn test_bad_discid() {
        let disc = lookup("dees besta zeker ni");
        assert!(disc.is_err());
    }
}
