use std::error::Error;

use minidom::Element;

use crate::data::{Disc, Track};

pub fn lookup(discid: &str) -> Result<Disc, Box<dyn Error>> {
    let lookup = format!("https://musicbrainz.org/ws/2/discid/{}", discid);
    let body: String = ureq::get(lookup.as_str()).call()?.into_string()?;
    let release = parse_disc(body)?;
    let body: String = ureq::get(release.as_str()).call()?.into_string()?;
    parse_metadata(body)
}

fn parse_disc(body: String) -> Result<String, Box<dyn Error>> {
    let metadata: minidom::Element = body.parse()?;
    let disc = metadata.children().next();
    if disc.is_some() {
        let disc = disc.unwrap();
        let release_list = disc.get_child("release-list", "http://musicbrainz.org/ns/mmd-2.0#");
        if release_list.is_some() {
            let release = release_list
                .unwrap()
                .get_child("release", "http://musicbrainz.org/ns/mmd-2.0#");
            if release.is_some() {
                let release_id = release.unwrap().attr("id").unwrap();
                let release = format!(
                    "https://musicbrainz.org/ws/2/release/{}?inc=%20recordings+artist-credits",
                    release_id
                );
                return Ok(release);
            }
        }
    }
    return Err("Failed to parse disc".into());
}

fn parse_metadata(xml: String) -> Result<Disc, Box<dyn Error>> {
    let metadata: minidom::Element = xml.parse()?;
    let release = metadata.children().next();
    if release.is_some() {
        let release = release.unwrap();
        let mut disc = Disc {
            ..Default::default()
        };
        disc.title = release
            .get_child("title", "http://musicbrainz.org/ns/mmd-2.0#")
            .unwrap()
            .text();

        disc.artist = get_artist(release).unwrap_or("".to_owned());

        let medium_list = release.get_child("medium-list", "http://musicbrainz.org/ns/mmd-2.0#");
        let medium = medium_list.unwrap().children().next().unwrap();
        let track_list = medium
            .get_child("track-list", "http://musicbrainz.org/ns/mmd-2.0#")
            .unwrap();
        for track in track_list.children() {
            let mut dtrack = Track {
                ..Default::default()
            };
            if track.has_child("number", "http://musicbrainz.org/ns/mmd-2.0#") {
                dtrack.number = track
                    .get_child("number", "http://musicbrainz.org/ns/mmd-2.0#")
                    .unwrap()
                    .text()
                    .parse()
                    .unwrap_or(0);
            }
            if track.has_child("recording", "http://musicbrainz.org/ns/mmd-2.0#") {
                let recording = track
                    .get_child("recording", "http://musicbrainz.org/ns/mmd-2.0#")
                    .unwrap();
                if recording.has_child("title", "http://musicbrainz.org/ns/mmd-2.0#") {
                    dtrack.title = recording
                        .get_child("title", "http://musicbrainz.org/ns/mmd-2.0#")
                        .unwrap()
                        .text();

                    dtrack.artist = get_artist(recording).unwrap_or("".to_owned());
                }
            }
            disc.tracks.push(dtrack);
        }
        return Ok(disc);
    }
    return Err("Failed to parse metadata".into());
}

fn get_artist(element: &Element) -> Option<String> {
    let artist_credit = element.get_child("artist-credit", "http://musicbrainz.org/ns/mmd-2.0#")?;
    let name_credit =
        artist_credit.get_child("name-credit", "http://musicbrainz.org/ns/mmd-2.0#")?;
    let artist = name_credit.get_child("artist", "http://musicbrainz.org/ns/mmd-2.0#")?;
    return Some(
        artist
            .get_child("name", "http://musicbrainz.org/ns/mmd-2.0#")?
            .text(),
    );
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
