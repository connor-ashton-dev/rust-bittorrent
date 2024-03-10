use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    iter::Peekable,
    net::TcpStream,
};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_urlencoded;
use sha1::{Digest, Sha1};

fn parse_ben_string<'a>(iter: &mut Peekable<std::slice::Iter<'a, u8>>) -> String {
    let mut length_str = Vec::new();
    loop {
        let char = iter.next().unwrap();
        if *char == b':' {
            break;
        }
        length_str.push(*char);
    }
    let length = String::from_utf8(length_str)
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut string = String::with_capacity(length);
    for _ in 0..length {
        string.push(*iter.next().unwrap() as char);
    }
    string
}

fn parse_ben_int<'a>(iter: &mut Peekable<std::slice::Iter<'a, u8>>) -> serde_json::Value {
    iter.next(); // Skip the 'i'
    let mut num_str = Vec::new();
    loop {
        let char = iter.next().unwrap();
        if *char == b'e' {
            break;
        }
        num_str.push(*char);
    }
    let num = String::from_utf8(num_str).unwrap().parse::<i64>().unwrap();
    serde_json::Value::Number(serde_json::Number::from(num))
}

fn parse_ben_list<'a>(iter: &mut Peekable<std::slice::Iter<'a, u8>>) -> serde_json::Value {
    iter.next(); // Skip the 'l'
    let mut items = Vec::new();
    loop {
        match iter.peek() {
            Some(&b'e') => {
                iter.next(); // Consume the 'e'
                break;
            }
            _ => items.push(decode_bencoded_value(iter)),
        }
    }
    serde_json::Value::Array(items)
}

fn parse_ben_dict<'a>(iter: &mut Peekable<std::slice::Iter<'a, u8>>) -> serde_json::Value {
    iter.next(); // Skip the 'd'
    let mut map = HashMap::new();
    loop {
        match iter.peek() {
            Some(&b'e') => {
                iter.next(); // Consume the 'e'
                break;
            }
            Some(_) => {
                let key = parse_ben_string(iter);
                let value = decode_bencoded_value(iter);
                map.insert(key, value);
            }
            None => panic!("Invalid dictionary format"),
        }
    }
    serde_json::json!(map)
}

fn decode_bencoded_value<'a>(iter: &mut Peekable<std::slice::Iter<'a, u8>>) -> serde_json::Value {
    let mut iter_clone = iter.clone();
    match iter_clone.peek() {
        Some(&byte) if byte.is_ascii_digit() => {
            let string = parse_ben_string(iter);
            serde_json::Value::String(string)
        }
        Some(&b'i') => parse_ben_int(iter),
        Some(&b'l') => parse_ben_list(iter),
        Some(&b'd') => parse_ben_dict(iter),
        _ => panic!("Invalid format"),
    }
}

#[derive(Deserialize, Serialize)]
struct TorrentFile {
    announce: String,
    info: TorrentFileInfo,
}

#[derive(Deserialize, Serialize)]
struct TorrentFileInfo {
    length: usize,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: usize,
    pieces: ByteBuf,
}

fn urlencode(t: &[u8; 20]) -> String {
    let mut encoded = String::with_capacity(3 * t.len());
    for &byte in t {
        encoded.push('%');
        encoded.push_str(&hex::encode(&[byte]));
    }
    encoded
}

#[derive(Serialize)]
struct QueryParams {
    peer_id: String,
    port: usize,
    uploaded: usize,
    downloaded: usize,
    left: usize,
    compact: usize,
}

#[derive(Deserialize)]
struct TrackerResponse {
    interval: usize,
    peers: ByteBuf,
}

struct Handshake {
    length_p_string: usize,
    p_string: String,
    reserved_bytes: Vec<u8>,
    sha1_infohash: Vec<u8>,
    peer_id: Vec<u8>,
}

fn parse_ips(ips: &[u8]) -> Vec<String> {
    ips.chunks(6)
        .map(|chunk| {
            let ip = format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            format!("{ip}:{port}")
        })
        .collect()
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2].clone().into_bytes();
        let mut iter = encoded_value.iter().peekable();
        let decoded_value = decode_bencoded_value(&mut iter);
        println!("{decoded_value}");
        Ok(())
    } else if command == "info" {
        let file_name = &args[2];
        let bytes = fs::read(file_name).unwrap();

        let torrent: TorrentFile = serde_bencode::from_bytes(&bytes)?;
        let encoded_info = serde_bencode::to_bytes(&torrent.info)?;

        let mut hasher = Sha1::new();
        hasher.update(encoded_info);
        let hash = hasher.finalize();

        println!(
            "Tracker URL: {}\nLength: {}\nInfo Hash: {}\nPiece Length: {}\nPiece Hashes:",
            torrent.announce,
            torrent.info.length,
            hex::encode(hash),
            torrent.info.piece_length,
        );
        for hash in torrent.info.pieces.chunks_exact(20) {
            println!("{}", hex::encode(hash));
        }
        Ok(())
    } else if command == "peers" {
        let file_name = &args[2];
        let bytes = fs::read(file_name)?;

        let torrent: TorrentFile = serde_bencode::from_bytes(&bytes)?;
        let info_encoded = serde_bencode::to_bytes(&torrent.info)?;

        let request: QueryParams = QueryParams {
            peer_id: "00112233445566778899".to_string(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.piece_length,
            compact: 1,
        };

        let url_params = serde_urlencoded::to_string(&request)?;

        let mut hasher = Sha1::new();
        hasher.update(&info_encoded);
        let info_hash: [u8; 20] = hasher.finalize().try_into()?;

        let tracker_url = format!(
            "{}?{}&info_hash={}",
            torrent.announce,
            url_params,
            &urlencode(&info_hash)
        );

        let res = reqwest::blocking::get(tracker_url)?;
        let body = res.bytes()?;
        let decoded: TrackerResponse = serde_bencode::from_bytes(&body)?;
        let peers = parse_ips(&decoded.peers);
        for peer in peers {
            println!("{peer}");
        }

        Ok(())
    } else if command == "handshake" {
        let file_name = &args[2];
        let bytes = fs::read(file_name)?;

        let torrent: TorrentFile = serde_bencode::from_bytes(&bytes)?;
        let info_encoded = serde_bencode::to_bytes(&torrent.info)?;

        let peer = &args[3];

        let p_string = "BitTorrent protocol";
        let reserved_bytes = [0; 8];
        let peer_id = "00112233445566778899".as_bytes();

        let mut hasher = Sha1::new();
        hasher.update(&info_encoded);
        let info_hash: [u8; 20] = hasher.finalize().try_into()?;

        let handshake = Handshake {
            length_p_string: p_string.len(),
            p_string: p_string.to_string(),
            reserved_bytes: reserved_bytes.into(),
            sha1_infohash: info_hash.into(),
            peer_id: peer_id.into(),
        };

        let mut handshake_bytes = Vec::new();
        handshake_bytes.push(handshake.length_p_string as u8);
        handshake_bytes.extend(handshake.p_string.as_bytes());
        handshake_bytes.extend(&handshake.reserved_bytes);
        handshake_bytes.extend(&handshake.sha1_infohash);
        handshake_bytes.extend(&handshake.peer_id);

        let mut stream = TcpStream::connect(peer)?;

        stream.write_all(&handshake_bytes)?;

        let mut response = [0; 68];
        stream.read_exact(&mut response)?;

        let length_p_string = response[0] as usize;
        let peer_id = response[length_p_string + 29..length_p_string + 49].to_vec();

        println!("Peer ID: {}", hex::encode(peer_id));

        Ok(())
    } else if command == "download_piece" {
        let output_path = &args[3];
        let file_name = &args[4];
        let piece_index: usize = args[5].parse()?;

        // 1. Read the torrent file
        let bytes = fs::read(file_name)?;
        let torrent: TorrentFile = serde_bencode::from_bytes(&bytes)?;

        let tracker_url = &torrent.announce;
        let info = &torrent.info;
        let piece_length = info.piece_length;
        let total_length = info.length;
        let num_pieces = (total_length + piece_length - 1) / piece_length;

        let this_piece_length = if piece_index < num_pieces - 1 {
            piece_length
        } else {
            total_length - (num_pieces - 1) * piece_length
        };

        let info_encoded = serde_bencode::to_bytes(&info)?;
        let mut hasher = Sha1::new();
        hasher.update(&info_encoded);
        let info_hash: [u8; 20] = hasher.finalize().try_into()?;

        // 2. Perform the tracker request

        let request: QueryParams = QueryParams {
            peer_id: "00112233445566778899".to_string(),
            port: 6881,
            uploaded: 0,
            downloaded: 0,
            left: torrent.info.piece_length,
            compact: 1,
        };

        let url_params = serde_urlencoded::to_string(&request)?;

        let tracker_url = format!(
            "{}?{}&info_hash={}",
            torrent.announce,
            url_params,
            &urlencode(&info_hash)
        );

        let res = reqwest::blocking::get(tracker_url)?;
        let body = res.bytes()?;
        let decoded: TrackerResponse = serde_bencode::from_bytes(&body)?;
        let peers = parse_ips(&decoded.peers);

        // 3. Establish a connection with a peer and perform the handshake

        let peer = &peers[0];
        let p_string = "BitTorrent protocol";
        let reserved_bytes = [0; 8];
        let peer_id = "00112233445566778899".as_bytes();

        let mut hasher = Sha1::new();
        hasher.update(&info_encoded);
        let handshake = Handshake {
            length_p_string: p_string.len(),
            p_string: p_string.to_string(),
            reserved_bytes: reserved_bytes.into(),
            sha1_infohash: info_hash.into(),
            peer_id: peer_id.into(),
        };

        let mut handshake_bytes = Vec::new();
        handshake_bytes.push(handshake.length_p_string as u8);
        handshake_bytes.extend(handshake.p_string.as_bytes());
        handshake_bytes.extend(&handshake.reserved_bytes);
        handshake_bytes.extend(&handshake.sha1_infohash);
        handshake_bytes.extend(&handshake.peer_id);

        let mut stream = TcpStream::connect(peer)?;

        stream.write_all(&handshake_bytes)?;

        let mut response = [0; 68];
        stream.read_exact(&mut response)?;

        // 4. Exchange peer messages to request and download a piece
        let mut buffer = vec![0; 4];
        stream.read_exact(&mut buffer)?;
        let message_length =
            u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

        if message_length > 0 {
            buffer.resize(message_length, 0);
            stream.read(&mut buffer)?;

            // check for bitfield message
        }

        // send interested message
        let interested = [0u8, 0, 0, 1, 2];
        stream.write_all(&interested)?;

        // wait for unchoke message
        loop {
            buffer.resize(4, 0);
            stream.read_exact(&mut buffer)?;
            let message_length =
                u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

            if message_length == 0 {
                continue;
            }

            buffer.resize(message_length, 0);
            stream.read_exact(&mut buffer)?;

            if buffer[0] == 1 {
                println!("Unchoked!");
                break;
            }
        }

        // request blocks within the piece
        let num_blocks = (this_piece_length + (16 * 1024) - 1) / (16 * 1024);
        for block_index in 0..num_blocks {
            let block_begin = block_index * 16 * 1024;
            let block_length = if block_index < num_blocks - 1 {
                16 * 1024
            } else {
                this_piece_length - block_begin
            };

            let request_message = [
                0u8,
                0,
                0,
                13,
                6, // 13 is the length of the message, 6 is the request message ID
                (piece_index >> 24) as u8,
                (piece_index >> 16) as u8,
                (piece_index >> 8) as u8,
                piece_index as u8,
                (block_begin >> 24) as u8,
                (block_begin >> 16) as u8,
                (block_begin >> 8) as u8,
                block_begin as u8,
                (block_length >> 24) as u8,
                (block_length >> 16) as u8,
                (block_length >> 8) as u8,
                block_length as u8,
            ];

            stream.write_all(&request_message)?;
            println!("Requested block {block_index} of piece {piece_index}");
        }

        // 5. Write the piece to the output file
        let mut piece_data = vec![0; this_piece_length];
        let mut blocks_received = 0;

        while blocks_received < num_blocks {
            let mut length_prefix = [0u8; 4];
            stream.read_exact(&mut length_prefix)?;
            let message_length = u32::from_be_bytes(length_prefix) as usize;

            if message_length == 0 {
                continue;
            }

            let mut message = vec![0u8; message_length];
            stream.read_exact(&mut message)?;
            if message[0] == 7 {
                // If it's a piece message
                let index =
                    u32::from_be_bytes([message[1], message[2], message[3], message[4]]) as usize;
                let begin =
                    u32::from_be_bytes([message[5], message[6], message[7], message[8]]) as usize;
                let block_data = &message[9..]; // The rest of the message is the block data

                // Ensure the piece and block data fits within the bounds of piece_data
                if begin + block_data.len() <= piece_data.len() {
                    piece_data[begin..begin + block_data.len()].copy_from_slice(block_data);
                    blocks_received += 1;
                    println!("Received block {blocks_received}/{num_blocks} for piece {index}");
                } else {
                    println!("Error: Block data exceeds piece data bounds.");
                }
            }
        }

        let piece_hash = Sha1::digest(&piece_data);
        let expected_hash = &torrent.info.pieces[piece_index * 20..(piece_index + 1) * 20];
        if piece_hash.as_slice() == expected_hash {
            match fs::write(output_path, &piece_data) {
                Ok(_) => println!("Piece {piece_index} successfully downloaded and verified"),
                Err(e) => println!("Error writing piece {piece_index} to file: {e}"),
            }
        } else {
            println!("Piece {piece_index} failed verification");
        }

        Ok(())
        // Assuming command == "download", similar structure to your existing code
    } else if command == "download" {
        let output_path = &args[3];
        let file_name = &args[4];

        // Read the torrent file
        let bytes = fs::read(file_name)?;
        let torrent: TorrentFile = serde_bencode::from_bytes(&bytes)?;

        // Initialize an empty vector to hold the downloaded file data
        let mut file_data = Vec::new();

        // Calculate the number of pieces
        let num_pieces = torrent.info.pieces.len() / 20; // Each hash is 20 bytes

        for piece_index in 0..num_pieces {
            // 1. Read the torrent file
            let info = &torrent.info;
            let piece_length = info.piece_length;
            let total_length = info.length;
            let num_pieces = (total_length + piece_length - 1) / piece_length;

            let this_piece_length = if piece_index < num_pieces - 1 {
                piece_length
            } else {
                total_length - (num_pieces - 1) * piece_length
            };

            let info_encoded = serde_bencode::to_bytes(&info)?;
            let mut hasher = Sha1::new();
            hasher.update(&info_encoded);
            let info_hash: [u8; 20] = hasher.finalize().try_into()?;

            // 2. Perform the tracker request

            let request: QueryParams = QueryParams {
                peer_id: "00112233445566778899".to_string(),
                port: 6881,
                uploaded: 0,
                downloaded: 0,
                left: torrent.info.piece_length,
                compact: 1,
            };

            let url_params = serde_urlencoded::to_string(&request)?;

            let tracker_url = format!(
                "{}?{}&info_hash={}",
                torrent.announce,
                url_params,
                &urlencode(&info_hash)
            );

            let res = reqwest::blocking::get(tracker_url)?;
            let body = res.bytes()?;
            let decoded: TrackerResponse = serde_bencode::from_bytes(&body)?;
            let peers = parse_ips(&decoded.peers);

            // 3. Establish a connection with a peer and perform the handshake

            let peer = &peers[0];
            let p_string = "BitTorrent protocol";
            let reserved_bytes = [0; 8];
            let peer_id = "00112233445566778899".as_bytes();

            let mut hasher = Sha1::new();
            hasher.update(&info_encoded);
            let handshake = Handshake {
                length_p_string: p_string.len(),
                p_string: p_string.to_string(),
                reserved_bytes: reserved_bytes.into(),
                sha1_infohash: info_hash.into(),
                peer_id: peer_id.into(),
            };

            let mut handshake_bytes = Vec::new();
            handshake_bytes.push(handshake.length_p_string as u8);
            handshake_bytes.extend(handshake.p_string.as_bytes());
            handshake_bytes.extend(&handshake.reserved_bytes);
            handshake_bytes.extend(&handshake.sha1_infohash);
            handshake_bytes.extend(&handshake.peer_id);

            let mut stream = TcpStream::connect(peer)?;

            stream.write_all(&handshake_bytes)?;

            let mut response = [0; 68];
            stream.read_exact(&mut response)?;

            // 4. Exchange peer messages to request and download a piece
            let mut buffer = vec![0; 4];
            stream.read_exact(&mut buffer)?;
            let message_length =
                u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

            if message_length > 0 {
                buffer.resize(message_length, 0);
                stream.read(&mut buffer)?;

                // check for bitfield message
            }

            // send interested message
            let interested = [0u8, 0, 0, 1, 2];
            stream.write_all(&interested)?;

            // wait for unchoke message
            loop {
                buffer.resize(4, 0);
                stream.read_exact(&mut buffer)?;
                let message_length =
                    u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;

                if message_length == 0 {
                    continue;
                }

                buffer.resize(message_length, 0);
                stream.read_exact(&mut buffer)?;

                if buffer[0] == 1 {
                    println!("Unchoked!");
                    break;
                }
            }

            // request blocks within the piece
            let num_blocks = (this_piece_length + (16 * 1024) - 1) / (16 * 1024);
            for block_index in 0..num_blocks {
                let block_begin = block_index * 16 * 1024;
                let block_length = if block_index < num_blocks - 1 {
                    16 * 1024
                } else {
                    this_piece_length - block_begin
                };

                let request_message = [
                    0u8,
                    0,
                    0,
                    13,
                    6, // 13 is the length of the message, 6 is the request message ID
                    (piece_index >> 24) as u8,
                    (piece_index >> 16) as u8,
                    (piece_index >> 8) as u8,
                    piece_index as u8,
                    (block_begin >> 24) as u8,
                    (block_begin >> 16) as u8,
                    (block_begin >> 8) as u8,
                    block_begin as u8,
                    (block_length >> 24) as u8,
                    (block_length >> 16) as u8,
                    (block_length >> 8) as u8,
                    block_length as u8,
                ];

                stream.write_all(&request_message)?;
                println!("Requested block {block_index} of piece {piece_index}");
            }

            // 5. Write the piece to the output file
            let mut piece_data = vec![0; this_piece_length];
            let mut blocks_received = 0;

            while blocks_received < num_blocks {
                let mut length_prefix = [0u8; 4];
                stream.read_exact(&mut length_prefix)?;
                let message_length = u32::from_be_bytes(length_prefix) as usize;

                if message_length == 0 {
                    continue;
                }

                let mut message = vec![0u8; message_length];
                stream.read_exact(&mut message)?;
                if message[0] == 7 {
                    // If it's a piece message
                    let index = u32::from_be_bytes([message[1], message[2], message[3], message[4]])
                        as usize;
                    let begin = u32::from_be_bytes([message[5], message[6], message[7], message[8]])
                        as usize;
                    let block_data = &message[9..]; // The rest of the message is the block data

                    // Ensure the piece and block data fits within the bounds of piece_data
                    if begin + block_data.len() <= piece_data.len() {
                        piece_data[begin..begin + block_data.len()].copy_from_slice(block_data);
                        blocks_received += 1;
                        println!("Received block {blocks_received}/{num_blocks} for piece {index}");
                    } else {
                        println!("Error: Block data exceeds piece data bounds.");
                    }
                }
            }

            let piece_hash = Sha1::digest(&piece_data);
            let expected_hash = &torrent.info.pieces[piece_index * 20..(piece_index + 1) * 20];
            if piece_hash.as_slice() == expected_hash {
                println!("Piece {piece_index} successfully downloaded and verified");
            } else {
                println!("Piece {piece_index} failed verification");
            }
            file_data.extend(piece_data);
        }

        // Once all pieces are downloaded and verified, save the combined file data to disk
        fs::write(output_path, file_data)?;
        println!("Downloaded {} to {}.", file_name, output_path);

        Ok(())
    } else {
        Err(anyhow!("Command not found: {}", command))
    }
}
