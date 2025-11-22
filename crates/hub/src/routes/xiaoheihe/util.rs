use captura_common::Result;
use md5::{Digest, Md5};
use rand_core::{OsRng, RngCore};
use url::Url;

/// 小黑盒 API 的 URL 签名算法，实现自 RSSHub `xiaoheihe/util.ts`。
///
/// 该函数会为给定 URL 添加 `hkey`、`_time` 和 `nonce` 查询参数。
pub fn calculate(url: &str) -> Result<String> {
    let timestamp = current_timestamp();
    let nonce = random_nonce();
    calculate_with_params(url, timestamp, &nonce)
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_secs() as i64
}

fn random_nonce() -> String {
    // 使用 16 字节随机数，经 md5 后转为大写 hex。
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    let digest = Md5::digest(&buf);
    hex::encode(digest).to_uppercase()
}

fn md5_hex_lower(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    hex::encode(digest)
}

const DICT: &str = "JKMNPQRTX1234OABCDFG56789H";

fn checksum(data: [u8; 4]) -> u8 {
    fn convert_byte(v: u8) -> u8 {
        if v & 0x80 != 0 {
            ((v << 1) ^ 0x1B) & 0xFF
        } else {
            v << 1
        }
    }

    fn c3(v: u8) -> u8 {
        convert_byte(v) ^ v
    }
    fn c2(v: u8) -> u8 {
        c3(convert_byte(v))
    }
    fn c1(v: u8) -> u8 {
        c2(c3(convert_byte(v)))
    }
    fn c0(v: u8) -> u8 {
        c1(v) ^ c2(v) ^ c3(v)
    }

    let [a, b, c, d] = data;
    let v0 = c0(a) ^ c1(b) ^ c2(c) ^ c3(d);
    let v1 = c3(a) ^ c0(b) ^ c1(c) ^ c2(d);
    let v2 = c2(a) ^ c3(b) ^ c0(c) ^ c1(d);
    let v3 = c1(a) ^ c2(b) ^ c3(c) ^ c0(d);
    let sum = v0 as u16 + v1 as u16 + v2 as u16 + v3 as u16;
    (sum % 100) as u8
}

fn calculate_with_params(url: &str, timestamp: i64, nonce: &str) -> Result<String> {
    let mut ts = timestamp;
    if ts <= 0 {
        ts = current_timestamp();
    }

    // pathname 处理：始终以 / 开头和结尾，中间无多余斜杠。
    let parsed = Url::parse(url).map_err(|e| captura_common::Error::Config(e.to_string()))?;
    let path = parsed.path();
    let normalized_path = {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", trimmed)
        }
    };

    let ts_plus = ts + 1;

    // 构造 nonceHash。
    let mut nonce_plus = String::with_capacity(nonce.len() + DICT.len());
    nonce_plus.push_str(nonce);
    nonce_plus.push_str(DICT);
    let digits_only: String = nonce_plus.chars().filter(|c| c.is_ascii_digit()).collect();
    let nonce_hash = md5_hex_lower(&digits_only);

    // rnd：md5(ts_plus + normalized_path + nonceHash)，取 hex 中的数字前 9 位，不足补 0。
    let mut rnd_src = String::new();
    rnd_src.push_str(&ts_plus.to_string());
    rnd_src.push_str(&normalized_path);
    rnd_src.push_str(&nonce_hash);
    let rnd_hex = md5_hex_lower(&rnd_src);
    let mut rnd_digits: String = rnd_hex.chars().filter(|c| c.is_ascii_digit()).collect();
    if rnd_digits.len() > 9 {
        rnd_digits.truncate(9);
    }
    while rnd_digits.len() < 9 {
        rnd_digits.push('0');
    }

    let mut c: i64 = rnd_digits.parse().unwrap_or(0);
    let dict_chars: Vec<char> = DICT.chars().collect();
    let dict_len = dict_chars.len() as i64;

    let mut key = String::new();
    for _ in 0..5 {
        let idx = (c % dict_len) as usize;
        c /= dict_len;
        key.push(dict_chars[idx]);
    }

    // 使用 key 最后 4 个字符计算校验和。
    let last4: Vec<u8> = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .map(|ch| ch as u8)
        .collect();
    let suffix = if last4.len() == 4 {
        format!("{:02}", checksum([last4[0], last4[1], last4[2], last4[3]]))
    } else {
        "00".to_string()
    };

    // 将 hkey/_time/nonce 追加到原 URL 的查询参数中。
    let mut url_obj = Url::parse(url).map_err(|e| captura_common::Error::Config(e.to_string()))?;

    {
        let mut pairs = url_obj.query_pairs_mut();
        pairs.append_pair("hkey", &format!("{}{}", key, suffix));
        pairs.append_pair("_time", &ts.to_string());
        pairs.append_pair("nonce", nonce);
    }

    Ok(url_obj.into_string())
}
