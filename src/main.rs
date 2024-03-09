use std::env;

// Available if you need it!
// use serde_bencode

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    // If encoded_value starts with a digit, it's a number
    if encoded_value.chars().next().unwrap().is_ascii_digit() {
        // Example: "5:hello" -> "hello"
        let colon_index = encoded_value.find(':').unwrap();
        let number_string = &encoded_value[..colon_index];
        let number = number_string.parse::<i64>().unwrap();
        let string =
            &encoded_value[colon_index + 1..colon_index + 1 + usize::try_from(number).unwrap()];
        serde_json::Value::String(string.to_string())
    } else if encoded_value.starts_with('i') && encoded_value.ends_with('e') {
        let num_str = &encoded_value[1..encoded_value.find('e').unwrap()];
        let num = num_str.parse::<i64>().unwrap();
        serde_json::json!(num)
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
