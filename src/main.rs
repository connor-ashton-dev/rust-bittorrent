use std::{collections::HashMap, env, fs, iter::Peekable};

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
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

#[derive(Serialize, Deserialize, Debug)]
struct TorrentInfo {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2].clone().into_bytes();
        let mut iter = encoded_value.iter().peekable();
        let decoded_value = decode_bencoded_value(&mut iter);
        println!("{decoded_value}");
    } else if command == "info" {
        let file_name = &args[2];
        let bytes = fs::read(file_name).unwrap();
        let mut iter = bytes.iter().peekable();
        let decoded_value = decode_bencoded_value(&mut iter);
        let info = decoded_value["info"].clone();

        let encoded_info = serde_bencode::to_string(&info).unwrap();

        let mut hasher = Sha1::new();
        hasher.update(encoded_info);
        let hash = hasher.finalize();

        println!(
            "Tracker URL: {}\nLength: {}\nInfo Hash: {}",
            decoded_value["announce"].as_str().unwrap(),
            decoded_value["info"]["length"],
            hex::encode(hash)
        );
    } else {
        println!("unknown command: {}", args[1]);
    }
}
