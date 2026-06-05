use anyhow::Result;
use encoding_rs::{Encoding, GBK, UTF_8};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use std::time::Duration;

pub struct HttpClient {
    client: reqwest::Client,
}

impl Clone for HttpClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}

impl HttpClient {
    pub fn new() -> Result<Self> {
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
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(10))
            .danger_accept_invalid_certs(true)
            .build()?;

        Ok(Self { client })
    }

    /// Fetch a URL and return the response as a string, auto-detecting encoding
    pub async fn get(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).send().await?;
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let bytes = resp.bytes().await?;
        self.decode_response(&bytes, &content_type)
    }

    /// Fetch with specific encoding
    pub async fn get_with_encoding(
        &self,
        url: &str,
        encoding: &'static Encoding,
    ) -> Result<String> {
        let resp = self.client.get(url).send().await?;
        let bytes = resp.bytes().await?;
        let (text, _, _) = encoding.decode(&bytes);
        Ok(text.into_owned())
    }

    /// Fetch a URL with custom headers
    pub async fn get_with_headers(&self, url: &str, headers: HeaderMap) -> Result<String> {
        let resp = self.client.get(url).headers(headers).send().await?;
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        let bytes = resp.bytes().await?;
        self.decode_response(&bytes, &content_type)
    }

    /// POST request with form data
    pub async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String> {
        let resp = self.client.post(url).form(form).send().await?;
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        let bytes = resp.bytes().await?;
        self.decode_response(&bytes, &content_type)
    }

    /// POST request with form data and specific encoding
    pub async fn post_form_with_encoding(
        &self,
        url: &str,
        form: &[(&str, &str)],
        encoding: &'static Encoding,
    ) -> Result<String> {
        let resp = self.client.post(url).form(form).send().await?;
        let bytes = resp.bytes().await?;
        let (text, _, _) = encoding.decode(&bytes);
        Ok(text.into_owned())
    }

    /// Fetch raw bytes
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.client.get(url).send().await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Get the underlying reqwest client for advanced usage
    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }

    fn decode_response(&self, bytes: &[u8], content_type: &str) -> Result<String> {
        // 1. Check Content-Type header for charset
        if let Some(encoding) = self.encoding_from_content_type(content_type) {
            let (text, _, _) = encoding.decode(bytes);
            return Ok(text.into_owned());
        }

        // 2. Try UTF-8 first
        if let Ok(text) = std::str::from_utf8(bytes) {
            return Ok(text.to_string());
        }

        // 3. Check HTML meta tag for charset
        if let Some(encoding) = self.encoding_from_html_meta(bytes) {
            let (text, _, _) = encoding.decode(bytes);
            return Ok(text.into_owned());
        }

        // 4. Fall back to GBK (common for Chinese sites)
        let (text, _, _) = GBK.decode(bytes);
        Ok(text.into_owned())
    }

    fn encoding_from_content_type(&self, content_type: &str) -> Option<&'static Encoding> {
        if content_type.contains("utf-8") || content_type.contains("utf8") {
            return Some(UTF_8);
        }
        if content_type.contains("gbk")
            || content_type.contains("gb2312")
            || content_type.contains("gb18030")
        {
            return Some(GBK);
        }
        if content_type.contains("big5") {
            return Some(encoding_rs::BIG5);
        }
        if content_type.contains("euc-jp") {
            return Some(encoding_rs::EUC_JP);
        }
        if content_type.contains("shift_jis") || content_type.contains("shift-jis") {
            return Some(encoding_rs::SHIFT_JIS);
        }
        if content_type.contains("euc-kr") {
            return Some(encoding_rs::EUC_KR);
        }
        None
    }

    fn encoding_from_html_meta(&self, bytes: &[u8]) -> Option<&'static Encoding> {
        // Quick scan of first 4096 bytes for meta charset
        let scan_len = bytes.len().min(4096);
        let partial = String::from_utf8_lossy(&bytes[..scan_len]).to_lowercase();

        if partial.contains("charset=gbk")
            || partial.contains("charset=gb2312")
            || partial.contains("charset=\"gbk\"")
            || partial.contains("charset=\"gb2312\"")
        {
            return Some(GBK);
        }
        if partial.contains("charset=big5") || partial.contains("charset=\"big5\"") {
            return Some(encoding_rs::BIG5);
        }
        if partial.contains("charset=euc-jp") {
            return Some(encoding_rs::EUC_JP);
        }
        if partial.contains("charset=shift_jis") || partial.contains("charset=shift-jis") {
            return Some(encoding_rs::SHIFT_JIS);
        }
        None
    }
}
