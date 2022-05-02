// id=ZDiPhVnBWu4wjogok6g2cGpgeNQ-
// DiscId 1 9 186755 150 18230 42558 57591 76417 89846 115065 143250 164582

use std::{
    io::{BufRead, BufReader, Write},
    net::{Shutdown, TcpStream},
};

use discid::DiscId;

use crate::data::Disc;

pub fn search_disc(discid: &DiscId) -> Result<Disc, String> {
    freedb(discid);
    Ok(Disc {
        ..Default::default()
    })
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

fn freedb(discid: &DiscId) {
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
            let response = send_command(&mut stream, query).unwrap();

            if response.starts_with("200") {
                // exact match
                let category = response.split(" ").nth(1).unwrap();
                let get = format!("cddb read {} {}\n", category, discid.freedb_id());
                let data = read_disc(&mut stream, get).unwrap();
                println!("disc:{}", data);
            } else {
            }
            stream.shutdown(Shutdown::Both).unwrap();
        }
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
        let offsets = [
            185700, 150, 18051, 42248, 57183, 75952, 89333, 114384, 142453, 163641,
        ];
        let disc = DiscId::put(1, &offsets).unwrap();
        println!("freedb id: {}", disc.freedb_id());
        println!("mb id: {}", disc.id());
        freedb(&disc);
    }
}
