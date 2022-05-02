// id=ZDiPhVnBWu4wjogok6g2cGpgeNQ-
// DiscId 1 9 186755 150 18230 42558 57591 76417 89846 115065 143250 164582

use std::{net::{TcpStream, Shutdown}, io::{Write, BufReader, BufRead}};

use discid::DiscId;

use crate::data::Disc;

pub fn search_disc(discid: &DiscId) -> Result<Disc, String> {
    freedb(discid);
    Ok(Disc{..Default::default()})
}

fn freedb(discid: &DiscId) {
    match TcpStream::connect("gnudb.gnudb.org:8880") {
        Ok(mut stream) => {
            println!("Successfully connected to server in port 8880");
            let mut hello = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut hello).unwrap();
            let msg = b"cddb hello ripperx localhost ripperx 4\n";

            stream.write(msg).unwrap();
            println!("Sent Hello, awaiting reply...");
            let mut response = String::new();
            match reader.read_line(&mut response) {
                Ok(_) => {
                    if response.starts_with("200") {
                        println!("{}", response);
                        // logged in
                        let count = discid.last_track_num() - discid.first_track_num() + 1;
                        let mut toc = discid.toc_string();
                        toc = toc.match_indices(" ").nth(2).map(|(index, _)| toc.split_at(index)).unwrap().1.to_owned();
                        let query = format!("cddb query {} {} {} {}\n", discid.freedb_id(), count, toc, discid.sectors() / 75);
                        let msg = query.as_bytes();
                        stream.write(msg).unwrap();
                        println!("Sent query {}, awaiting reply...", query);
                        response = String::new();
                        match reader.read_line(&mut response) {
                            Ok(_) => {
                                println!("{}", response);
                                if response.starts_with("200") {
                                    // exact match
                                    let category = response.split(" ").nth(1).unwrap();
                                    let get = format!("cddb read {} {}\n", category, discid.freedb_id());
                                    println!("sent {}", get);
                                    let msg = get.as_bytes();
                                    stream.write(msg).unwrap();
                                    loop {
                                        response = String::new();
                                        let result = reader.read_line(&mut response);
                                        match result {
                                            Ok(_) => {
                                                print!("{}", response);
                                                if response.starts_with(".") {
                                                    // done
                                                    break;
                                                } else {
                                                    
                                                }
                                            },
                                            Err(e) => {
                                                println!("Failed to receive data: {}", e);
                                            }
                                        }
                                    }
                                } else {
                                }
                            },
                            Err(e) => {
                                println!("Failed to receive data: {}", e);
                            }
                        }
                        // failed
                    }
                },
                Err(e) => {
                    println!("Failed to receive data: {}", e);
                }
            }
            stream.shutdown(Shutdown::Both).unwrap();
        },
        Err(e) => {
            println!("Failed to connect: {}", e);
        }
    }
    println!("Terminated.");
}
#[cfg(test)]
mod test {
    use discid::DiscId;

    use crate::metadata::freedb;

    #[test]
    fn test_freedb() {
        let offsets = [185700, 150, 18051, 42248, 57183, 75952, 89333, 114384, 142453, 163641];
        let disc = DiscId::put(1, &offsets).unwrap();
        println!("freedb id: {}", disc.freedb_id());
        println!("mb id: {}", disc.id());
        freedb(&disc);
    }

}
