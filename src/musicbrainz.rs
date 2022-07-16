fn musicbrainz(discid: String) -> String {
    let lookup = "https://musicbrainz.org/ws/2/discid/xA3p59dQpJpDXZYHz1SSQ491oaU-";
    let release = "https://musicbrainz.org/ws/2/release/a541c6e6-eb8c-4fb2-b0bb-5c07e89c2182?inc=%20recordings";
    let body: String = ureq::get(release)
        .call().unwrap()
        .into_string().unwrap();
    body
}

#[cfg(test)]
mod test {
    use super::musicbrainz;

    #[test]
    fn test_parse() {
        let body = musicbrainz("xA3p59dQpJpDXZYHz1SSQ491oaU-".to_owned());
        println!("{}", body);
    }
}