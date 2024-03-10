use std::{collections::HashMap, env, fs, iter::Peekable};

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

fn bencode_value(value: &serde_json::Value) -> Vec<u8> {
    match value {
        serde_json::Value::String(s) => {
            let length = s.len().to_string();
            [length.as_bytes(), b":", s.as_bytes()].concat()
        }
        serde_json::Value::Number(n) => {
            let n_str = n.to_string();
            [b"i", n_str.as_bytes(), b"e"].concat()
        }
        serde_json::Value::Array(arr) => {
            let mut encoded = Vec::new();
            encoded.push(b'l');
            for item in arr {
                encoded.extend_from_slice(&bencode_value(item));
            }
            encoded.push(b'e');
            encoded
        }
        serde_json::Value::Object(obj) => {
            let mut encoded = Vec::new();
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort(); // Bencode dictionaries should have their keys sorted lexicographically
            encoded.push(b'd');
            for key in keys {
                encoded.extend_from_slice(&bencode_value(&serde_json::Value::String(key.clone())));
                encoded.extend_from_slice(&bencode_value(obj.get(key).unwrap()));
            }
            encoded.push(b'e');
            encoded
        }
        _ => panic!("Unsupported type"),
    }
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

        // Bencode the info dictionary
        let bencoded_info = bencode_value(&info);

        // Calculate the SHA-1 hash of the bencoded info dictionary
        let mut hasher = Sha1::new();
        hasher.update(&bencoded_info);
        let hash_result = hasher.finalize();

        println!(
            "Tracker URL: {}\nLength: {}\nInfo Hash: {:x}",
            decoded_value["announce"].as_str().unwrap(),
            decoded_value["info"]["length"],
            hash_result,
        );
    } else {
        println!("unknown command: {}", args[1]);
    }
}
