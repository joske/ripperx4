// id=ZDiPhVnBWu4wjogok6g2cGpgeNQ-
// DiscId 1 9 186755 150 18230 42558 57591 76417 89846 115065 143250 164582

use std::{
    io::{BufRead, BufReader, Write},
    net::{Shutdown, TcpStream},
};

use discid::DiscId;

use crate::data::{Disc, Track};

pub fn search_disc(discid: &DiscId) -> Result<Disc, String> {
    freedb(discid)
}

fn send_command(stream: &mut TcpStream, cmd: String) -> Result<String, String> {
    let msg = cmd.as_bytes();
    stream.write(msg).unwrap();
    println!("sent {}", cmd);
    let mut response = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    match reader.read_line(&mut response) {
        Ok(_) => {
            println!("response: {}", response);
            if response.starts_with("200") {
                Ok(response)
            } else {
                Err(response)
            }
        }
        Err(e) => {
            println!("Failed to send command: {}", e);
            Err(e.to_string())
        }
    }
}

fn cddb_query(stream: &mut TcpStream, cmd: String, discid: &DiscId) -> Result<Disc, String> {
    let msg = cmd.as_bytes();
    stream.write(msg).unwrap();
    println!("sent {}", cmd);
    let mut response = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    match reader.read_line(&mut response) {
        Ok(_) => {
            println!("response: {}", response);
            if response.starts_with("200") {
                // exact match
                let category = response.split(" ").nth(1).unwrap();
                let disc = cddb_read(category, discid.freedb_id().as_str(), stream);
                stream.shutdown(Shutdown::Both).unwrap();
                return Ok(disc);
            } else if response.starts_with("211") {
                // inexact match - we take first hit for now
                response = String::new();
                reader.read_line(&mut response).unwrap(); // we expect at least one extra line
                let mut split = response.split(" ");
                let category = split.next().unwrap();
                let discid = split.next().unwrap();
                let disc = cddb_read(category, discid, stream);
                stream.shutdown(Shutdown::Both).unwrap();
                return Ok(disc);
            } else {
                stream.shutdown(Shutdown::Both).unwrap();
                return Err("failed to query disc".to_owned());
            }            
        }
        Err(e) => {
            println!("Failed to send command: {}", e);
            Err(e.to_string())
        }
    }
}

fn read_disc(stream: &mut TcpStream, cmd: String) -> Result<String, String> {
    let msg = cmd.as_bytes();
    stream.write(msg).unwrap();
    println!("sent {}", cmd);
    let mut data = String::new();
    let mut response = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    loop {
        let result = reader.read_line(&mut response);
        match result {
            Ok(_) => {
                if response.starts_with(".") {
                    // done
                    break;
                } else {
                    data.push_str(response.as_str());
                    response = String::new();
                }
            }
            Err(e) => {
                println!("Failed to receive data: {}", e);
                return Err("failed to read".to_owned());
            }
        }
    }
    Ok(data)
}

fn freedb(discid: &DiscId) -> Result<Disc, String> {
    match TcpStream::connect("gnudb.gnudb.org:8880") {
        Ok(mut stream) => {
            println!("Successfully connected to server in port 8880");
            let mut hello = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut hello).unwrap();
            let hello = "cddb hello ripperx localhost ripperx 4\n".to_owned();
            send_command(&mut stream, hello).unwrap();
            let count = discid.last_track_num() - discid.first_track_num() + 1;
            let mut toc = discid.toc_string();
            toc = toc
                .match_indices(" ")
                .nth(2)
                .map(|(index, _)| toc.split_at(index))
                .unwrap()
                .1
                .to_owned();
            let query = format!(
                "cddb query {} {} {} {}\n",
                discid.freedb_id(),
                count,
                toc,
                discid.sectors() / 75
            );
            let disc = cddb_query(&mut stream, query, discid);

            stream.shutdown(Shutdown::Both).unwrap();
            return disc;
        }
        Err(e) => {
            println!("Failed to connect: {}", e);
        }
    }
    println!("Terminated.");
    return Err("".to_owned());
}

fn cddb_read(category: &str, discid: &str, stream: &mut TcpStream) -> Disc {
    let get = format!("cddb read {} {}\n", category, discid);
    let data = read_disc(stream, get).unwrap();
    let disc = parse_data(data);
    println!("disc:{:?}", disc);
    disc
}

fn parse_data(data: String) -> Disc {
    println!("{}", data);
    let mut disc = Disc {
        ..Default::default()
    };
    let mut i = 0;
    for ref line in data.lines() {
        if line.starts_with("DTITLE") {
            let value = line.split("=").nth(1).unwrap();
            let mut split = value.split("/");
            disc.artist = split.next().unwrap().trim().to_owned();
            disc.title = split.next().unwrap().trim().to_owned();
        }
        if line.starts_with("DYEAR") {
            let value = line.split("=").nth(1).unwrap();
            disc.year = value.parse::<u16>().unwrap();
        }
        if line.starts_with("TTITLE") {
            let mut track = Track {
                ..Default::default()
            };
            track.number = i + 1;
            track.title = line.split("=").nth(1).unwrap().to_owned();
            track.artist = disc.artist.clone();
            disc.tracks.push(track);
            i += 1;
        }
    }
    disc
}
#[cfg(test)]
mod test {
    use discid::DiscId;

    use crate::metadata::freedb;

    #[test]
    fn test_freedb() {
        let offsets = [
            185700, 150, 18051, 42248, 57183, 75952, 89333, 114384, 142453, 163641,
        ];
        let discid = DiscId::put(1, &offsets).unwrap();
        println!("freedb id: {}", discid.freedb_id());
        println!("mb id: {}", discid.id());
        let disc = freedb(&discid);
        assert!(disc.is_ok());
    }
}
