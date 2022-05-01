// id=ZDiPhVnBWu4wjogok6g2cGpgeNQ-
// DiscId 1 9 186755 150 18230 42558 57591 76417 89846 115065 143250 164582

use musicbrainz_rs::entity::{release::{ReleaseSearchQuery}};

use crate::data::Disc;

pub fn search_disc(discid: &str) -> Result<Disc, String> {
    let query = ReleaseSearchQuery::query_builder().discids(discid).build();
    println!("query: {:?}", query);
    // Artist::(query).execute().unwrap();
    // let result = Release::search(query).execute().unwrap();
    // print!("result={:?}", result);
    Ok(Disc{..Default::default()})
}

#[cfg(test)]
mod test {
    use discid::DiscId;

    use super::search_disc;

    #[test]
    fn test_search_dire_straits() {
        let discid = "ZDiPhVnBWu4wjogok6g2cGpgeNQ-";
        search_disc(discid).unwrap();
    }

    #[test]
    fn test_freedb() {
        let offsets = [185700, 150, 18051, 42248, 57183, 75952, 89333, 114384, 142453, 163641];
        let disc = DiscId::put(1, &offsets).unwrap();
        println!("freedb id: {}", disc.freedb_id());
        println!("mb id: {}", disc.id());
    }

}
