use rust_embed::RustEmbed;
use std::collections::HashMap;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct I18nAssets;

fn parse_dict(bytes: &[u8]) -> HashMap<String, String> {
    serde_json::from_slice::<HashMap<String, String>>(bytes).unwrap_or_default()
}

pub fn load(lang: &str) -> HashMap<String, String> {
    // fallback to en_US
    let mut base = if let Some(f) = I18nAssets::get("en_US.json") {
        parse_dict(&f.data)
    } else {
        HashMap::new()
    };
    if lang != "en_US" {
        let fname = format!("{}.json", lang);
        if let Some(f) = I18nAssets::get(&fname) {
            let overlay = parse_dict(&f.data);
            base.extend(overlay);
        }
    }
    base
}
