use std::{collections::HashMap, env, iter::Peekable, str::Chars};

fn parse_ben_string<'a>(iter: &mut Peekable<Chars<'a>>) -> String {
    let mut length_str = String::new();
    let mut char = iter.next().unwrap();
    while char != ':' {
        length_str.push(char);
        char = iter.next().unwrap();
    }

    let length = length_str.parse::<usize>().unwrap();
    let mut string = String::with_capacity(length);

    for _ in 0..length {
        string.push(iter.next().unwrap());
    }

    string
}

fn parse_ben_int<'a>(iter: &mut Peekable<Chars<'a>>) -> serde_json::Value {
    iter.next(); // Skip the 'i'
    let mut num_str = String::new();
    let mut char = iter.next().unwrap();
    while char != 'e' {
        num_str.push(char);
        char = iter.next().unwrap();
    }
    let num = num_str.parse::<i64>().unwrap();
    serde_json::Value::Number(serde_json::Number::from(num))
}

fn parse_ben_list<'a>(iter: &mut Peekable<Chars<'a>>) -> serde_json::Value {
    iter.next(); // Skip the 'l'
    let mut items = Vec::new();

    loop {
        match iter.peek() {
            Some('e') => {
                iter.next(); // Consume the 'e'
                break;
            }
            _ => items.push(decode_bencoded_value(iter)),
        }
    }

    serde_json::Value::Array(items)
}

fn parse_ben_dict<'a>(iter: &mut Peekable<Chars<'a>>) -> serde_json::Value {
    iter.next(); // Skip the 'd'
    let mut map = HashMap::new();

    loop {
        match iter.peek() {
            Some('e') => {
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

fn decode_bencoded_value<'a>(iter: &mut Peekable<Chars<'a>>) -> serde_json::Value {
    let mut iter_clone = iter.clone();
    match iter_clone.peek() {
        Some(c) if c.is_ascii_digit() => {
            let string = parse_ben_string(iter);
            serde_json::Value::String(string)
        }
        Some('i') => parse_ben_int(iter),
        Some('l') => parse_ben_list(iter),
        Some('d') => parse_ben_dict(iter),
        _ => panic!("Invalid format"),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let mut iter = encoded_value.chars().peekable();
        let decoded_value = decode_bencoded_value(&mut iter);
        println!("{decoded_value}");
    } else {
        println!("unknown command: {}", args[1]);
    }
}
