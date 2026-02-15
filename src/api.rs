use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, REFERER, ORIGIN};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchResponse {
    pub data: Vec<Anime>,
    pub last_page: u32,
    pub current_page: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Anime {
    pub id: u32,
    pub title: String,
    pub session: String,
    pub episodes: Option<u32>,
    pub score: Option<f64>,
    pub status: String,
    pub year: Option<u32>,
    #[serde(rename = "type")]
    pub anime_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SeriesResponse {
    pub title: String,
    pub episodes: Vec<Episode>,
    pub total_pages: u32,
    pub page: u32,
    pub next: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Episode {
    pub episode: String,
    pub session: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StreamItem {
    pub link: String,
    pub name: String,
}

pub struct AnimeClient {
    client: reqwest::Client,
    base_url: String,
}

impl AnimeClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));
        headers.insert(ORIGIN, HeaderValue::from_static("https://www.animepah.me"));
        headers.insert(REFERER, HeaderValue::from_static("https://www.animepah.me/"));
        
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: "https://anime.apex-cloud.workers.dev".to_string(),
        })
    }

    pub async fn search(&self, query: &str) -> Result<SearchResponse> {
        let url = format!("{}/?method=search&query={}", self.base_url, query);
        let resp = self.client.get(&url).send().await?;
        let text = resp.text().await?;
        serde_json::from_str(&text).context("Failed to parse search response")
    }

    pub async fn get_episodes(&self, session: &str, page: u32) -> Result<SeriesResponse> {
        let url = format!("{}/?method=series&session={}&page={}", self.base_url, session, page);
        let resp = self.client.get(&url).send().await?;
        let text = resp.text().await?;
        serde_json::from_str(&text).context("Failed to parse episodes response")
    }

    pub async fn get_stream(&self, series_session: &str, episode_session: &str) -> Result<Vec<StreamItem>> {
        let url = format!("{}/?method=episode&session={}&ep={}", self.base_url, series_session, episode_session);
        let resp = self.client.get(&url).send().await?;
        let text = resp.text().await?;
        serde_json::from_str(&text).context("Failed to parse stream response")
    }

    pub async fn extract_stream_url(&self, kwik_url: &str) -> Result<String> {
        let mut kwik_headers = HeaderMap::new();
        kwik_headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));
        kwik_headers.insert(REFERER, HeaderValue::from_static("https://kwik.cx/"));
        
        let kwik_client = reqwest::Client::builder()
            .default_headers(kwik_headers)
            .build()
            .context("Failed to build kwik client")?;

        let f_page = kwik_client.get(kwik_url).send().await?.text().await?;
        
        // Find the embed pathSlug slug
        let slug_re = Regex::new("/f/([a-zA-Z0-9]+)")?;
        let slug = slug_re.captures(kwik_url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .context("Could not extract slug from kwik URL")?;
        
        let embed_url = self.decode_kwik_f_page(&f_page, slug)?;
        let embed_page_url = format!("https://kwik.cx{}", embed_url);
        let e_page = kwik_client.get(&embed_page_url)
            .header(REFERER, kwik_url)
            .send().await?.text().await?;
        
        let stream_url = self.decode_kwik_embed_page(&e_page)?;
        Ok(stream_url)
    }

    fn decode_kwik_f_page(&self, html: &str, _slug: &str) -> Result<String> {
        if let Some(decoded) = self.unpack_custom_kwik(html)? {
            // Regex to find the embed URL in the decoded JS
            let url_re = Regex::new(r#"var\s+url\s*=\s*'(/e/[^']+)'"#)?;
            if let Some(url_match) = url_re.captures(&decoded) {
                return Ok(url_match.get(1).unwrap().as_str().to_string());
            }
            
            // Sometimes it's directly the m3u8? (Unlikely on /f/ page)
            if let Some(m3u8) = self.extract_m3u8(&decoded) {
                return Ok(m3u8);
            }
        }
        
        // Fallback or old method
        let url_re = Regex::new(r#"https://kwik\.cx/e/[a-zA-Z0-9]+"#)?;
        if let Some(m) = url_re.find(html) {
            return Ok(m.as_str().replace("https://kwik.cx", ""));
        }

        bail!("Could not find embed URL in kwik /f/ page")
    }

    fn decode_kwik_embed_page(&self, html: &str) -> Result<String> {
        // Many pages now use the same custom obfuscator as the /f/ page
        if let Some(decoded) = self.unpack_custom_kwik(html)? {
            if let Some(m3u8) = self.extract_m3u8(&decoded) {
                return Ok(m3u8);
            }
        }

        // More lenient regex for packer that handles nested braces
        let packer_re = Regex::new(r#"(?s)eval\(function\(p,a,c,k,e,d\)\{.*?\}\('(.*?)',(\d+),(\d+),'(.*?)'\.split\('([|\\\\])'\),\d+,\{\}\)\)"#)?;
        
        for caps in packer_re.captures_iter(html) {
            let packed = caps.get(1).unwrap().as_str();
            let base = caps.get(2).unwrap().as_str().parse::<usize>()?;
            let keywords_str = caps.get(4).unwrap().as_str();
            let separator = caps.get(5).unwrap().as_str();
            let keywords: Vec<&str> = keywords_str.split(separator).collect();
            
            let decoded = self.unpack_dean_edwards(packed, base, &keywords)?;
            
            if let Some(m3u8) = self.extract_m3u8(&decoded) {
                return Ok(m3u8);
            }
        }
        bail!("Could not find m3u8 URL in kwik embed page")
    }

    fn unpack_custom_kwik(&self, html: &str) -> Result<Option<String>> {
        // Pattern: eval(function(a,b,c,d,e,f){...}("...", 19, "...", 9, 2, 32))
        // We make the variable names generic \w+
        let eval_re = Regex::new(r#"(?s)eval\(function\(\w+,\w+,\w+,\w+,\w+,\w+\)\{.*?\}\("(?P<cipher>[^"]+)",\s*(?P<my>\d+),\s*"(?P<mu>[^"]+)",\s*(?P<bu>\d+),\s*(?P<fo>\d+),\s*(?P<zn>\d+)\)\)"#)?;
        
        if let Some(caps) = eval_re.captures(html) {
            let encoded_data = caps.name("cipher").unwrap().as_str();
            let charset = caps.name("mu").unwrap().as_str();
            let offset = caps.name("bu").unwrap().as_str().parse::<i64>()?;
            let radix = caps.name("fo").unwrap().as_str().parse::<u32>()?;

            let charset_chars: Vec<char> = charset.chars().collect();
            let separator = charset_chars[radix as usize];
            
            let mut decoded_bytes = Vec::new();
            let segments: Vec<&str> = encoded_data.split(separator).collect();
            
            for segment in segments {
                if segment.is_empty() { continue; }
                
                let mut decimal: u128 = 0;
                for ch in segment.chars() {
                    if let Some(pos) = charset_chars.iter().position(|&c| c == ch) {
                        decimal = decimal * (radix as u128) + (pos as u128);
                    }
                }
                
                let char_code = (decimal as i128) - (offset as i128);
                if char_code >= 0 && char_code <= 255 {
                    decoded_bytes.push(char_code as u8);
                }
            }
            
            let decoded_str = String::from_utf8_lossy(&decoded_bytes).to_string();
            // The JS does decodeURIComponent(escape(zN))
            // decoded_bytes is already the result of escape(zN) mapping if we treat them as bytes.
            return Ok(Some(decoded_str));
        }
        Ok(None)
    }

    fn extract_m3u8(&self, text: &str) -> Option<String> {
        let m3u8_re = Regex::new(r#"https?://[^'"]+\.m3u8"#).unwrap();
        m3u8_re.find(text).map(|m| m.as_str().to_string())
    }

    fn unpack_dean_edwards(&self, packed: &str, base: usize, keywords: &[&str]) -> Result<String> {
        let chars = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let word_re = Regex::new("\\b\\w+\\b")?;
        
        let result = word_re.replace_all(packed, |caps: &regex::Captures| {
            let token = caps.get(0).unwrap().as_str();
            let mut value: usize = 0;
            let mut valid = true;
            for ch in token.chars() {
                if let Some(pos) = chars.find(ch) {
                    if pos >= base { valid = false; break; }
                    value = value * base + pos;
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && value < keywords.len() && !keywords[value].is_empty() {
                keywords[value].to_string()
            } else {
                token.to_string()
            }
        });
        Ok(result.to_string())
    }
}
