use std::{env, iter::Peekable, str::Chars};

// Available if you need it!
// use serde_bencode

fn parse_ben_string(encoded_value: &str) -> serde_json::Value {
    let colon_index = encoded_value.find(':').unwrap();
    let number_string = &encoded_value[..colon_index];
    let number = number_string.parse::<i64>().unwrap();
    let string =
        &encoded_value[colon_index + 1..colon_index + 1 + usize::try_from(number).unwrap()];
    serde_json::Value::String(string.to_string())
}

fn parse_ben_int(encoded_value: &str) -> serde_json::Value {
    let num_str = &encoded_value[1..encoded_value.find('e').unwrap()];
    let num = num_str.parse::<i64>().unwrap();
    serde_json::Value::Number(serde_json::Number::from(num))
}

fn parse_ben_list(iter: &mut Peekable<Chars<'_>>) -> serde_json::Value {
    let mut items: Vec<serde_json::Value> = vec![];

    while let Some(c) = iter.next() {
        // String
        if c.is_ascii_digit() && iter.peek().unwrap() == &':' {
            // get length of word
            let length = c.to_digit(10).unwrap();

            // skip colon
            iter.next();

            // build string
            let mut res = String::new();
            for _ in 0..length {
                let new_char = iter.next().unwrap();
                res.push(new_char);
            }
            items.push(serde_json::json!(res));

        // Integer
        } else if c == 'i' && iter.peek().unwrap().is_ascii_digit() {
            let mut res = String::new();
            let mut cur_char = iter.next().unwrap();
            while cur_char != 'e' {
                res.push(cur_char);
                cur_char = iter.next().unwrap();
            }
            let num = res.parse::<i64>().unwrap();
            items.push(serde_json::Value::Number(serde_json::Number::from(num)));

            // Another list
        } else if c == 'l' {
            let new_items = parse_ben_list(iter);
            items.push(new_items);
        } else if c == 'e' {
            return serde_json::json!(items);
        }
    }

    serde_json::json!(items)
}

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    let mut iter = encoded_value.chars().peekable();
    if encoded_value.chars().next().unwrap().is_ascii_digit() {
        parse_ben_string(encoded_value)
    } else if encoded_value.starts_with('i') && encoded_value.ends_with('e') {
        parse_ben_int(encoded_value)
    } else if encoded_value.starts_with('l') && encoded_value.ends_with('e') {
        iter.next();
        parse_ben_list(&mut iter)
    } else {
        panic!("Unhandled encoded value: {encoded_value}")
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{decoded_value}");
    } else {
        println!("unknown command: {}", args[1]);
    }
}
