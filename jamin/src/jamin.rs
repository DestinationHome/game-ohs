use super::value::*;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use std::char;
use std::collections::HashMap;

lazy_static! {
    static ref ESCAPE_REGEX: Regex = Regex::new(r#"#(#|(\d{3}))"#).unwrap();
    // Anchor the integer-array prefix and include the trailing ':' so captures end at the data start
    static ref INTARRAY_PREFIX_REGEX: Regex = Regex::new(r"^i(\d+),(\d+):").unwrap();

    // Build the same alphabet as the Lua implementation: chars 33..=126 excluding a small special set
    static ref LETTERS: Vec<char> = {
        let mut v: Vec<char> = Vec::new();
        let specials: [char; 8] = [' ', '\'', '"', '<', '>', '#', '%', '&'];
        for i in 33u32..=126u32 {
            if let Some(c) = char::from_u32(i)
                && !specials.contains(&c)
            {
                v.push(c);
            }
        }
        v
    };

    static ref INV_LETTERS: HashMap<char, usize> = {
        let mut m = HashMap::new();
        for (i, c) in LETTERS.iter().enumerate() {
            // Store 1-based index to mirror the Lua mapping (where letters[index+1])
            m.insert(*c, i + 1);
        }
        m
    };

    static ref NUM_LETTERS: usize = LETTERS.len();
}

const MIN_INTEGER_ARRAY_ELEMENT: i64 = -(1 << 24);
const MAX_INTEGER_ARRAY_ELEMENT: i64 = 1 << 24;

pub struct JaminDecoder;
impl JaminDecoder {
    fn decode_nil(payload: &str) -> Option<(Value, usize)> {
        if payload.starts_with("z;") {
            Some((Value::Nil, 2))
        } else {
            None
        }
    }

    fn decode_bool(payload: &str) -> Option<(Value, usize)> {
        if payload.starts_with("bt;") {
            Some((Value::Bool(true), 3))
        } else if payload.starts_with("bf;") {
            Some((Value::Bool(false), 3))
        } else {
            None
        }
    }

    fn parse_number_token(payload: &str) -> Option<(Value, usize)> {
        let data = payload
            .chars()
            .take_while(|c| {
                *c == '.' || *c == '-' || *c == '+' || *c == 'e' || *c == 'E' || c.is_numeric()
            })
            .collect::<String>();

        if data.is_empty() {
            return None;
        }

        let num = if data.chars().any(|c| c == '.' || c == 'e' || c == 'E') {
            data.parse::<f64>().map(Number::Float).ok()
        } else if data.chars().nth(0) == Some('-') {
            data.parse::<i64>().map(Number::Negative).ok()
        } else {
            data.parse::<u64>().map(Number::Positive).ok()
        };

        num.map(|n| (Value::Number(n), data.len()))
    }

    fn decode_number(payload: &str) -> Option<(Value, usize)> {
        if payload.chars().nth(0) != Some('n') {
            return None;
        }

        let (val, len) = Self::parse_number_token(&payload[1..])?;

        // Check for semicolon
        if payload.chars().nth(1 + len) != Some(';') {
            return None;
        }

        Some((val, len + 2))
    }

    fn decode_string(payload: &str) -> Option<(Value, usize)> {
        if payload.chars().nth(0) != Some('s') {
            return None;
        }

        let count = payload
            .chars()
            .skip(1)
            .take_while(|c| c.is_numeric())
            .collect::<String>()
            .parse::<usize>()
            .ok()?;

        let data_start = payload.chars().position(|c| c == ':')? + 1;

        if payload.chars().nth(data_start + count) != Some(';') {
            return None;
        }

        let data = payload[data_start..data_start + count].to_string();

        Some((
            Value::String(
                ESCAPE_REGEX
                    .replace_all(&data, |caps: &Captures| {
                        if caps.get(2).is_some() {
                            // This is a #ddd escape sequence
                            let code_str = caps.get(2).unwrap().as_str();
                            let code = code_str.parse::<u8>().unwrap();
                            (code as char).to_string()
                        } else {
                            // This is a ## escape sequence
                            "#".to_string()
                        }
                    })
                    .to_string(),
            ),
            data_start + count + 1,
        ))
    }

    fn decode_vector(payload: &str) -> Option<(Value, usize)> {
        if payload.chars().nth(0) != Some('v') {
            return None;
        }

        let mut result = Vector(
            Number::Positive(0),
            Number::Positive(0),
            Number::Positive(0),
            Number::Positive(0),
        );

        let mut start = 1usize;
        for i in 0..4 {
            if let Some((Value::Number(n), len)) = Self::parse_number_token(&payload[start..]) {
                result[i] = n;
                // Check delimiter: space for first 3, semicolon for last
                let delimiter = payload.chars().nth(start + len);
                if i < 3 {
                    if delimiter != Some(' ') {
                        return None;
                    }
                } else {
                    if delimiter != Some(';') {
                        return None;
                    }
                }
                start += len + 1;
            } else {
                return None;
            }
        }

        Some((Value::Vector(result), start))
    }

    fn decode_table(payload: &str) -> Option<(Value, usize)> {
        if payload.chars().nth(0) != Some('t') {
            return None;
        }

        let mut result: Option<TableType> = None;

        let mut pos = 1usize;
        loop {
            if payload.chars().nth(pos) == Some(';') {
                return Some((
                    Value::Table(result.unwrap_or_else(|| TableType::Array(Vec::new()))),
                    pos + 1,
                ));
            }

            let (key, ksz) = match Self::decode_value(&payload[pos..]) {
                Some((Value::String(k), sz)) => (Key::String(k), sz),
                Some((Value::Number(n), sz)) => (Key::try_from(n).map_err(|_| ()).ok()?, sz),
                _ => {
                    break None;
                }
            };
            pos += ksz;

            // Initialize the table type if it hasn't been initialized yet.
            if result.is_none() {
                if let Key::Positive(_) = key {
                    result = Some(TableType::Array(Vec::new()));
                } else {
                    result = Some(TableType::Map(HashMap::new()));
                }
            }
            let result = result.as_mut().unwrap();

            if let Some((value, vsz)) = Self::decode_value(&payload[pos..]) {
                match result {
                    TableType::Array(arr) => arr.push(value),
                    TableType::Map(map) => {
                        map.insert(key, value);
                    }
                }

                pos += vsz;
            } else {
                break None;
            }
        }
    }

    fn decode_integer_array(payload: &str) -> Option<(Value, usize)> {
        // Expect payload to start with the integer-array prefix like: i{wordlen},{numElements}:
        if let Some(caps) = INTARRAY_PREFIX_REGEX.captures(payload) {
            // Ensure the match is anchored at the start
            if let Some(mat) = caps.get(0) {
                if mat.start() != 0 {
                    return None;
                }

                let wordlen = caps.get(1).unwrap().as_str().parse::<usize>().ok()?;
                let num_elements = caps.get(2).unwrap().as_str().parse::<usize>().ok()?;

                let start = mat.end(); // byte index where encoded data begins
                let encoded_len = wordlen.checked_mul(num_elements)?;
                let finish = start + encoded_len; // byte index of the ';' expected

                // Check bounds
                if finish >= payload.len() {
                    return None;
                }

                if payload.as_bytes()[finish] != b';' {
                    return None;
                }

                let encoded = &payload[start..finish];

                let mut result: Vec<i64> = Vec::with_capacity(num_elements);
                let mut normed: i128 = 0;

                // Precompute base as i128
                let base = *NUM_LETTERS as i128;

                for (count, enc_ch) in encoded.chars().enumerate() {
                    let word_index = count % wordlen;

                    let inv = INV_LETTERS.get(&enc_ch)?; // 1-based
                    let digit = (inv - 1) as i128 * base.pow(word_index as u32);

                    normed += digit;

                    if wordlen > 0 && word_index == wordlen - 1 {
                        // Compute offset for this wordlen
                        let size = base.pow(wordlen as u32);
                        let offset = size / 2;

                        let value = (normed - offset) as i64;
                        result.push(value);
                        normed = 0;
                    }
                }

                return Some((Value::IntArray(result), finish + 1));
            }
        }

        None
    }

    pub fn decode_value(payload: &str) -> Option<(Value, usize)> {
        if let Some(value) = Self::decode_nil(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_bool(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_number(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_string(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_vector(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_table(payload) {
            return Some(value);
        }

        if let Some(value) = Self::decode_integer_array(payload) {
            return Some(value);
        }

        None
    }

    pub fn decode(payload: &str) -> Option<Value> {
        Self::decode_value(payload).map(|(x, _)| x)
    }
}

pub struct JaminEncoder;
impl JaminEncoder {
    fn encode_nil() -> String {
        "z;".to_string()
    }

    fn encode_bool(value: bool) -> String {
        format!("b{};", if value { "t" } else { "f" })
    }

    fn encode_number(value: Number) -> String {
        format!("n{};", value)
    }

    fn encode_string(value: &str) -> String {
        // First escape # to ##, then escape unsafe characters
        let escaped = value
            .chars()
            .map(|c| {
                if c == '#' {
                    "##".to_string()
                } else {
                    let byte = c as u8;
                    // Safe range: alphanumeric, punctuation, space (32-126 excluding special chars)
                    if (32..=126).contains(&byte)
                        && !matches!(c, '\'' | '"' | '<' | '>' | '%' | '&')
                    {
                        c.to_string()
                    } else {
                        format!("#{:03}", byte)
                    }
                }
            })
            .collect::<String>();

        format!("s{}:{};", escaped.len(), escaped)
    }

    fn encode_vector(value: &Vector) -> String {
        format!("v{} {} {} {};", value.0, value.1, value.2, value.3)
    }

    fn encode_table(value: &TableType) -> String {
        // 1. Transform arrays into maps, filtering out "Null" values to perfectly
        // mimic Lua's behavior where `nil` keys are entirely skipped by `pairs()`.
        let table: HashMap<Key, Value> = match value {
            TableType::Array(arr) => arr
                .iter()
                .enumerate()
                // NOTE: Adjust `Value::Null` to match your actual Enum variant name
                .filter(|(_, v)| !matches!(v, Value::Nil))
                .map(|(i, v)| (Key::Positive(i as u64 + 1), v.clone()))
                .collect(),
            TableType::Map(map) => map
                .iter()
                .filter(|(_, v)| !matches!(v, Value::Nil))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };

        let mut sorted_entries: Vec<_> = table.iter().collect();

        // 2. FIXED: Strict weak ordering.
        // Negative numbers < Positive numbers < Strings
        sorted_entries.sort_by(|(k1, _), (k2, _)| {
            match (k1, k2) {
                // Same type: natural ordering
                (Key::Positive(a), Key::Positive(b)) => a.cmp(b),
                (Key::Negative(a), Key::Negative(b)) => a.cmp(b),
                (Key::String(a), Key::String(b)) => a.cmp(b),

                // Cross-type ordering
                (Key::Negative(_), Key::Positive(_)) => std::cmp::Ordering::Less,
                (Key::Positive(_), Key::Negative(_)) => std::cmp::Ordering::Greater,

                (Key::Negative(_), Key::String(_)) | (Key::Positive(_), Key::String(_)) => {
                    std::cmp::Ordering::Less
                }
                (Key::String(_), Key::Negative(_)) | (Key::String(_), Key::Positive(_)) => {
                    std::cmp::Ordering::Greater
                }
            }
        });

        // 3. Formatting remains unchanged
        format!(
            "t{};",
            sorted_entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}{}",
                        match k {
                            Key::Positive(k) => Self::encode_number(Number::Positive(*k)),
                            Key::Negative(k) => Self::encode_number(Number::Negative(*k)),
                            Key::String(k) => Self::encode_string(k),
                        },
                        Self::encode(v)
                    )
                })
                .collect::<String>()
        )
    }

    fn calc_integer_array_word_length(array: &[i64]) -> usize {
        if array.is_empty() {
            return 0;
        }

        let mut minv: i64 = MAX_INTEGER_ARRAY_ELEMENT;
        let mut maxv: i64 = MIN_INTEGER_ARRAY_ELEMENT;

        for &v in array.iter() {
            assert!((MIN_INTEGER_ARRAY_ELEMENT..=MAX_INTEGER_ARRAY_ELEMENT).contains(&v));
            if v < minv {
                minv = v;
            }
            if v > maxv {
                maxv = v;
            }
        }

        let abs_max = std::cmp::max(minv.abs(), maxv.abs()) as u128;
        let mut normed_range = abs_max.saturating_mul(2u128);
        if normed_range == 0 {
            normed_range = 1;
        }

        let num_letters_f = *NUM_LETTERS as f64;
        let range_f = (normed_range as f64).max(1.0);

        let result = range_f.log(num_letters_f).ceil() as usize;
        // Ensure at least 1 when there are elements
        if result == 0 { 1 } else { result }
    }

    fn encode_integer_array(value: &[i64]) -> String {
        // Handle empty array explicitly
        if value.is_empty() {
            return "i1,0:;".to_string();
        }

        let wordlen = Self::calc_integer_array_word_length(value);
        let base = *NUM_LETTERS as i128;

        // Compute size/offset for the chosen wordlen
        let size = base.pow(wordlen as u32);
        let offset = size / 2; // floor(size/2)

        let mut out = String::new();
        out.push_str(&format!("i{},{}:", wordlen, value.len()));

        for &element in value.iter() {
            assert!((MIN_INTEGER_ARRAY_ELEMENT..=MAX_INTEGER_ARRAY_ELEMENT).contains(&element));

            let normed = (element as i128) + offset;
            assert!(0 <= normed && normed < size);

            for i in 0..wordlen {
                let index = ((normed / base.pow(i as u32)) % base) as usize;
                let ch = LETTERS[index];
                out.push(ch);
            }
        }

        out.push(';');
        out
    }

    pub fn encode(value: &Value) -> String {
        match value {
            Value::Nil => Self::encode_nil(),
            Value::Bool(x) => Self::encode_bool(*x),
            Value::Number(x) => Self::encode_number(*x),
            Value::String(x) => Self::encode_string(x),
            Value::Vector(x) => Self::encode_vector(x),
            Value::Table(x) => Self::encode_table(x),
            Value::IntArray(a) => Self::encode_integer_array(a),
        }
    }

    pub fn encode_serde<T: serde::Serialize>(value: &T) -> String {
        let json_value = serde_json::to_value(value).unwrap();
        Self::encode(&Self::serde_to_jamin(&json_value))
    }

    pub fn serde_to_jamin(v: &serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Nil,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => {
                if n.is_f64() {
                    Value::Number(Number::Float(n.as_f64().unwrap()))
                } else if n.is_i64() {
                    let i = n.as_i64().unwrap();
                    if i < 0 {
                        Value::Number(Number::Negative(i))
                    } else {
                        Value::Number(Number::Positive(i as u64))
                    }
                } else if n.is_u64() {
                    Value::Number(Number::Positive(n.as_u64().unwrap()))
                } else {
                    Value::Number(Number::Float(n.as_f64().unwrap()))
                }
            }
            serde_json::Value::String(s) => Value::String(s.clone()),
            serde_json::Value::Array(arr) => {
                let mut vec = Vec::with_capacity(arr.len());
                for e in arr.iter() {
                    vec.push(Self::serde_to_jamin(e));
                }
                Value::Table(TableType::Array(vec))
            }
            serde_json::Value::Object(obj) => {
                let mut map = std::collections::HashMap::new();
                for (k, v) in obj.iter() {
                    map.insert(Key::String(k.clone()), Self::serde_to_jamin(v));
                }
                Value::Table(TableType::Map(map))
            }
        }
    }
}
