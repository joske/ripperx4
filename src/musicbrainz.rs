fn musicbrainz(discid: &str) -> String {
    let lookup = format!("https://musicbrainz.org/ws/2/discid/{}", discid);
    let body: String = ureq::get(lookup.as_str())
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    let metadata: minidom::Element = body.parse().unwrap();
    let disc = metadata.children().next().unwrap();
    let release_list = disc
        .get_child("release-list", "http://musicbrainz.org/ns/mmd-2.0#");
    if release_list.is_some() {
        let release = release_list
            .unwrap()
            .get_child("release", "http://musicbrainz.org/ns/mmd-2.0#");
        if release.is_some() {
            let release_id = release.unwrap().attr("id").unwrap();
            let release = format!(
                "https://musicbrainz.org/ws/2/release/{}?inc=%20recordings",
                release_id
            );
            let body: String = ureq::get(release.as_str())
                .call()
                .unwrap()
                .into_string()
                .unwrap();
            return body;
        }
    }
    return "".to_owned();
}

#[cfg(test)]
mod test {
    use super::musicbrainz;

    #[test]
    fn test_parse() {
        let body = musicbrainz("xA3p59dQpJpDXZYHz1SSQ491oaU-");
        println!("{:?}", body);
    }
}
