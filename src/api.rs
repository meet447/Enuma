use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, LOCATION, ORIGIN, REFERER, USER_AGENT,
};
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;
use url::Url;

static SLUG_RE: OnceLock<Regex> = OnceLock::new();
static URL_RE: OnceLock<Regex> = OnceLock::new();
static KWIK_URL_RE: OnceLock<Regex> = OnceLock::new();
static KWIK_PATH_RE: OnceLock<Regex> = OnceLock::new();
static PACKER_RE: OnceLock<Regex> = OnceLock::new();
static EVAL_RE: OnceLock<Regex> = OnceLock::new();
static M3U8_RE: OnceLock<Regex> = OnceLock::new();
static WORD_RE: OnceLock<Regex> = OnceLock::new();
static FORM_ACTION_RE: OnceLock<Regex> = OnceLock::new();
static TOKEN_NAME_FIRST_RE: OnceLock<Regex> = OnceLock::new();
static TOKEN_VALUE_FIRST_RE: OnceLock<Regex> = OnceLock::new();
static META_CSRF_RE: OnceLock<Regex> = OnceLock::new();
static SCRIPT_ACTION_RE: OnceLock<Regex> = OnceLock::new();
static META_REFRESH_RE: OnceLock<Regex> = OnceLock::new();
static WINDOW_LOC_RE: OnceLock<Regex> = OnceLock::new();

const KWIK_ORIGIN: &str = "https://kwik.cx";

/// Merge two `Cookie` header values (e.g. manual `ENUMA_KWIK_COOKIES` + FlareSolverr cookies).
fn merge_cookie_headers(a: Option<&str>, b: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = [a, b]
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn kwik_cookie_header(
    req: reqwest::RequestBuilder,
    cookies: Option<&str>,
) -> reqwest::RequestBuilder {
    let Some(c) = cookies.map(|s| s.trim()).filter(|c| !c.is_empty()) else {
        return req;
    };
    match HeaderValue::from_str(c) {
        Ok(hv) => req.header(COOKIE, hv),
        Err(_) => req,
    }
}

/// Call a local [FlareSolverr](https://github.com/FlareSolverr/FlareSolverr) instance (same idea as
/// outsourcing Cloudflare to a headless browser, similar to how Node stacks combine `cloudscraper`
/// with real browser sessions when needed).
async fn flare_solve_get(
    http: &reqwest::Client,
    flare_base: &str,
    target_url: &str,
) -> Result<(String, Option<String>)> {
    let endpoint = format!("{}/v1", flare_base.trim_end_matches('/'));
    let payload = serde_json::json!({
        "cmd": "request.get",
        "url": target_url,
        "maxTimeout": 180000,
    });
    let resp = http
        .post(endpoint)
        .json(&payload)
        .timeout(Duration::from_secs(190))
        .send()
        .await
        .context("FlareSolverr: could not reach service (is it running?)")?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .context("FlareSolverr: response was not JSON")?;

    if body["status"].as_str() != Some("ok") {
        let msg = body["message"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("unknown error");
        bail!("FlareSolverr error (HTTP {status}): {msg}");
    }

    let solution = &body["solution"];
    let html = solution["response"]
        .as_str()
        .or_else(|| solution["body"].as_str())
        .context("FlareSolverr: solution.response missing")?
        .to_string();

    let cookie_line: Option<String> = solution["cookies"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let name = c["name"].as_str()?;
                    let value = c["value"].as_str()?;
                    Some(format!("{name}={value}"))
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|s| !s.trim().is_empty());

    Ok((html, cookie_line))
}

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
    kwik_client: reqwest::Client,
    base_url: &'static str,
}

impl AnimeClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));
        headers.insert(ORIGIN, HeaderValue::from_static("https://www.animepah.me"));
        headers.insert(
            REFERER,
            HeaderValue::from_static("https://www.animepah.me/"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        let kwik_client = Self::build_kwik_client()?;

        Ok(Self {
            client,
            kwik_client,
            base_url: "https://anime.apex-cloud.workers.dev",
        })
    }

    fn build_kwik_client() -> Result<reqwest::Client> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
            ),
        );
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

        reqwest::Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .redirect(Policy::none())
            .build()
            .context("Failed to build Kwik HTTP client")
    }

    pub async fn search(&self, query: &str) -> Result<SearchResponse> {
        let url = format!(
            "{}/?method=search&query={}",
            self.base_url,
            urlencoding::encode(query)
        );
        let resp = self.client.get(&url).send().await?;
        resp.json::<SearchResponse>()
            .await
            .context("Failed to parse search response")
    }

    pub async fn get_episodes(&self, session: &str, page: u32) -> Result<SeriesResponse> {
        let url = format!(
            "{}/?method=series&session={}&page={}",
            self.base_url,
            urlencoding::encode(session),
            page
        );
        let resp = self.client.get(&url).send().await?;
        resp.json::<SeriesResponse>()
            .await
            .context("Failed to parse episodes response")
    }

    pub async fn get_stream(
        &self,
        series_session: &str,
        episode_session: &str,
    ) -> Result<Vec<StreamItem>> {
        let url = format!(
            "{}/?method=episode&session={}&ep={}",
            self.base_url,
            urlencoding::encode(series_session),
            urlencoding::encode(episode_session)
        );
        let resp = self.client.get(&url).send().await?;
        resp.json::<Vec<StreamItem>>()
            .await
            .context("Failed to parse stream response")
    }

    pub async fn extract_stream_url(&self, kwik_url: &str) -> Result<String> {
        let slug_re = SLUG_RE.get_or_init(|| Regex::new("/f/([a-zA-Z0-9]+)").unwrap());
        let _slug = slug_re
            .captures(kwik_url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .context("Could not extract slug from kwik URL")?;

        let env_cookie = std::env::var("ENUMA_KWIK_COOKIES")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut cookie_header: Option<String> = env_cookie;

        let f_page = if let Ok(flare_raw) = std::env::var("ENUMA_KWIK_FLARESOLVERR_URL") {
            let flare_base = flare_raw.trim().trim_end_matches('/').to_string();
            let (html, from_flare) = flare_solve_get(&self.client, &flare_base, kwik_url)
                .await
                .context("FlareSolverr (ENUMA_KWIK_FLARESOLVERR_URL) failed")?;
            cookie_header = merge_cookie_headers(cookie_header.as_deref(), from_flare.as_deref());
            html
        } else {
            let cookies = cookie_header.as_deref();
            kwik_cookie_header(
                self.kwik_client
                    .get(kwik_url)
                    .header(REFERER, HeaderValue::from_static(KWIK_ORIGIN)),
                cookies,
            )
            .send()
            .await?
            .text()
            .await?
        };

        let cookies = cookie_header.as_deref();

        if is_cloudflare_interstitial(&f_page) {
            bail!(
                "kwik.cx returned a Cloudflare challenge (no real embed page). Options: \
                 (1) Set ENUMA_KWIK_COOKIES to your browser Cookie string for kwik.cx (include cf_clearance if present); \
                 (2) Run [FlareSolverr](https://github.com/FlareSolverr/FlareSolverr) locally and set ENUMA_KWIK_FLARESOLVERR_URL to its base URL (e.g. http://127.0.0.1:8191)."
            );
        }

        if let Ok(direct) = self.try_kwik_form_post(kwik_url, &f_page, cookies).await {
            return Ok(direct);
        }

        let embed_path = self.decode_kwik_f_page(&f_page)?;
        let embed_page_url = resolve_against(kwik_url, &embed_path)?;
        let e_page = kwik_cookie_header(
            self.kwik_client.get(&embed_page_url).header(
                REFERER,
                HeaderValue::from_str(kwik_url).unwrap_or(HeaderValue::from_static(KWIK_ORIGIN)),
            ),
            cookies,
        )
        .send()
        .await?
        .text()
        .await?;

        self.decode_kwik_embed_page(&e_page)
    }

    /// Matches [animepahe-api](https://github.com/ElijahCodes12345/animepahe-api) `getKwikDownloadUrl`:
    /// POST `_token` to the form action, then follow `Location` or parse the response body.
    async fn try_kwik_form_post(
        &self,
        page_url: &str,
        html: &str,
        cookies: Option<&str>,
    ) -> Result<String> {
        let Some((action, token)) = extract_kwik_form(html) else {
            bail!("no kwik form on page");
        };

        let post_url = resolve_against(page_url, &action)?;

        tokio::time::sleep(Duration::from_secs(2)).await;

        let resp = kwik_cookie_header(
            self.kwik_client
                .post(&post_url)
                .header(
                    REFERER,
                    HeaderValue::from_str(page_url)
                        .unwrap_or(HeaderValue::from_static(KWIK_ORIGIN)),
                )
                .header(ORIGIN, HeaderValue::from_static(KWIK_ORIGIN))
                .form(&[("_token", token.as_str())]),
            cookies,
        )
        .send()
        .await?;

        let status = resp.status();

        if matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ) {
            if let Some(loc) = resp.headers().get(LOCATION) {
                let loc_str = loc.to_str().context("redirect Location header")?;
                let target = resolve_against(&post_url, loc_str)?;
                if target.contains(".m3u8") {
                    return Ok(target);
                }
                let body = kwik_cookie_header(
                    self.kwik_client.get(&target).header(
                        REFERER,
                        HeaderValue::from_str(page_url)
                            .unwrap_or(HeaderValue::from_static(KWIK_ORIGIN)),
                    ),
                    cookies,
                )
                .send()
                .await?
                .text()
                .await?;
                return self.decode_kwik_embed_page(&body);
            }
        }

        if status.is_success() {
            let body = resp.text().await?;
            if let Some(u) = extract_redirect_from_html(&body) {
                let target = resolve_against(&post_url, &u)?;
                if target.contains(".m3u8") {
                    return Ok(target);
                }
                let inner = kwik_cookie_header(
                    self.kwik_client.get(&target).header(
                        REFERER,
                        HeaderValue::from_str(page_url)
                            .unwrap_or(HeaderValue::from_static(KWIK_ORIGIN)),
                    ),
                    cookies,
                )
                .send()
                .await?
                .text()
                .await?;
                return self.decode_kwik_embed_page(&inner);
            }
        }

        bail!("kwik form POST did not yield a stream URL")
    }

    fn decode_kwik_f_page(&self, html: &str) -> Result<String> {
        if let Some(decoded) = self.unpack_custom_kwik(html)? {
            let url_re = URL_RE
                .get_or_init(|| Regex::new(r#"var\s+url\s*=\s*['"](/e/[^'"]+)['"]"#).unwrap());
            if let Some(url_match) = url_re.captures(&decoded) {
                return Ok(url_match.get(1).unwrap().as_str().to_string());
            }

            if let Some(m3u8) = self.extract_m3u8(&decoded) {
                return Ok(m3u8);
            }
        }

        let kwik_url_re =
            KWIK_URL_RE.get_or_init(|| Regex::new(r#"https://kwik\.cx/e/[a-zA-Z0-9_-]+"#).unwrap());
        if let Some(m) = kwik_url_re.find(html) {
            return Ok(m.as_str().replace("https://kwik.cx", ""));
        }

        let path_re =
            KWIK_PATH_RE.get_or_init(|| Regex::new(r#"['"](/e/[a-zA-Z0-9_-]+)['"]"#).unwrap());
        if let Some(c) = path_re.captures(html) {
            return Ok(c.get(1).unwrap().as_str().to_string());
        }

        bail!("Could not find embed URL in kwik /f/ page")
    }

    fn decode_kwik_embed_page(&self, html: &str) -> Result<String> {
        if let Some(decoded) = self.unpack_custom_kwik(html)? {
            if let Some(m3u8) = self.extract_m3u8(&decoded) {
                return Ok(m3u8);
            }
        }

        if let Some(m3u8) = self.extract_m3u8(html) {
            return Ok(m3u8);
        }

        let packer_re = PACKER_RE.get_or_init(|| {
            Regex::new(r#"(?s)eval\(function\(p,a,c,k,e,d\)\{.*?\}\('(.*?)',(\d+),(\d+),'(.*?)'\.split\('([|\\\\])'\),\d+,\{\}\)\)"#).unwrap()
        });

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
        let eval_re = EVAL_RE.get_or_init(|| {
            Regex::new(
                r#"(?s)eval\(function\(\w+,\w+,\w+,\w+,\w+,\w+\)\{.*?\}\("(?P<cipher>[^"]+)",\s*(?P<my>\d+),\s*"(?P<mu>[^"]+)",\s*(?P<bu>\d+),\s*(?P<fo>\d+),\s*(?P<zn>\d+)\)\)"#,
            )
            .unwrap()
        });

        if let Some(caps) = eval_re.captures(html) {
            let encoded_data = caps.name("cipher").unwrap().as_str();
            let charset = caps.name("mu").unwrap().as_str();
            let offset = caps.name("bu").unwrap().as_str().parse::<i64>()?;
            let radix = caps.name("fo").unwrap().as_str().parse::<u32>()?;

            let charset_chars: Vec<char> = charset.chars().collect();
            let separator = charset_chars.get(radix as usize).copied().unwrap_or('|');

            let mut decoded_bytes = Vec::new();
            let segments: Vec<&str> = encoded_data.split(separator).collect();

            for segment in segments {
                if segment.is_empty() {
                    continue;
                }

                let mut decimal: u128 = 0;
                for ch in segment.chars() {
                    if let Some(pos) = charset_chars.iter().position(|&c| c == ch) {
                        decimal = decimal * (radix as u128) + (pos as u128);
                    }
                }

                let char_code = (decimal as i128) - (offset as i128);
                if (0..=255).contains(&char_code) {
                    decoded_bytes.push(char_code as u8);
                }
            }

            let decoded_str = String::from_utf8_lossy(&decoded_bytes).to_string();
            return Ok(Some(decoded_str));
        }
        Ok(None)
    }

    fn extract_m3u8(&self, text: &str) -> Option<String> {
        let m3u8_re =
            M3U8_RE.get_or_init(|| Regex::new(r#"https?://[^'"\s<>]+\.m3u8[^'"\s<>]*"#).unwrap());
        m3u8_re.find(text).map(|m| m.as_str().to_string())
    }

    fn unpack_dean_edwards(&self, packed: &str, base: usize, keywords: &[&str]) -> Result<String> {
        let chars = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let word_re = WORD_RE.get_or_init(|| Regex::new("\\b\\w+\\b").unwrap());

        let result = word_re.replace_all(packed, |caps: &regex::Captures| {
            let token = caps.get(0).unwrap().as_str();
            let mut value: usize = 0;
            let mut valid = true;
            for ch in token.chars() {
                if let Some(pos) = chars.find(ch) {
                    if pos >= base {
                        valid = false;
                        break;
                    }
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

fn is_cloudflare_interstitial(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("just a moment")
        || lower.contains("challenges.cloudflare.com")
        || lower.contains("cf-challenge")
        || lower.contains("enable javascript and cookies")
}

fn extract_kwik_form(html: &str) -> Option<(String, String)> {
    let form_action_re = FORM_ACTION_RE
        .get_or_init(|| Regex::new(r#"(?is)<form[^>]*\saction\s*=\s*["']([^"']+)["']"#).unwrap());
    let token_name_first = TOKEN_NAME_FIRST_RE.get_or_init(|| {
        Regex::new(r#"(?is)name\s*=\s*["']_token["'][^>]*value\s*=\s*["']([^"']+)["']"#).unwrap()
    });
    let token_value_first = TOKEN_VALUE_FIRST_RE.get_or_init(|| {
        Regex::new(r#"(?is)value\s*=\s*["']([^"']+)["'][^>]*name\s*=\s*["']_token["']"#).unwrap()
    });
    let meta_csrf = META_CSRF_RE.get_or_init(|| {
        Regex::new(r#"(?is)<meta\s+name\s*=\s*["']csrf-token["']\s+content\s*=\s*["']([^"']+)["']"#)
            .unwrap()
    });
    let script_action = SCRIPT_ACTION_RE.get_or_init(|| {
        Regex::new(r#"(?is)action\s*=\s*["'](https://kwik\.cx[^"']+)["']"#).unwrap()
    });

    let action = form_action_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .or_else(|| {
            script_action
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })?;

    let token = token_name_first
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .or_else(|| {
            token_value_first
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })
        .or_else(|| {
            meta_csrf
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })?;

    Some((action, token))
}

fn extract_redirect_from_html(body: &str) -> Option<String> {
    let meta = META_REFRESH_RE.get_or_init(|| {
        Regex::new(r#"(?is)<meta[^>]*http-equiv\s*=\s*["']refresh["'][^>]*content\s*=\s*["'][^"']*url\s*=\s*([^"';]+)"#).unwrap()
    });
    if let Some(c) = meta.captures(body) {
        return Some(c.get(1).unwrap().as_str().trim().to_string());
    }
    let win = WINDOW_LOC_RE.get_or_init(|| {
        Regex::new(r#"(?is)window\.location(?:\.href)?\s*=\s*["']([^"']+)["']"#).unwrap()
    });
    win.captures(body)
        .map(|c| c.get(1).unwrap().as_str().to_string())
}

fn resolve_against(base: &str, reference: &str) -> Result<String> {
    let b = Url::parse(base).with_context(|| format!("invalid base URL: {base}"))?;
    let joined = b
        .join(reference)
        .with_context(|| format!("join failed: {reference}"))?;
    Ok(joined.into())
}
