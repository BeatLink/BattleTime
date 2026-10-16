// Fetches URLs for watch apps, but only from hosts the user has allowed, as src/services/http-proxy.ts does.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use super::Daemon;
use crate::web_request::{request, CertificateTrust, WebReply, WebRequest};

// ============================================================================
// Limits
// ============================================================================

// The longest answer sent back, counted in UTF-16 units as the window counts it, since the watch has only a few kilobytes of heap.
pub const MOST_RESPONSE_UNITS: usize = 1024;

pub const HTTP_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

// How long a request may take, generous because some hosts answer slowly from here.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

// ============================================================================
// The web, behind a seam
// ============================================================================

// Carries out one web request; tests hand in their own so they need no network.
pub type WebFetch = Arc<dyn Fn(WebRequest) -> BoxFuture<'static, Result<WebReply, String>> + Send + Sync>;

// The real web, with each request run on a blocking thread.
pub fn web_fetch() -> WebFetch {
    Arc::new(|outgoing: WebRequest| {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || request(&outgoing))
                .await
                .map_err(|error| error.to_string())?
        })
    })
}

// A plain request with the system's certificates and no extra headers.
pub fn plain_request(method: &str, url: &str, body: Option<String>) -> WebRequest {
    WebRequest {
        method: method.to_string(),
        url: url.to_string(),
        headers: BTreeMap::new(),
        body,
        timeout_ms: REQUEST_TIMEOUT.as_millis() as u64,
        certificate: CertificateTrust::System,
        certificate_sha256: None,
    }
}

// ============================================================================
// Settings
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpSettings {
    pub http_enabled: bool,
    pub http_allowed_hosts: String,
}

// ============================================================================
// Rules
// ============================================================================

// A host is allowed when it, or a domain it belongs to, is on the list.
pub fn is_host_allowed(host: &str, allowed: &[String]) -> bool {
    let name = host.to_lowercase();
    allowed.iter().any(|entry| {
        let rule = entry.trim().to_lowercase();
        !rule.is_empty() && (name == rule || name.ends_with(&format!(".{rule}")))
    })
}

pub fn parse_host_list(text: &str) -> Vec<String> {
    text.split(|character: char| character.is_whitespace() || character == ',')
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .collect()
}

// The scheme and host of an absolute URL, both in lower case, or None when it is not one.
fn scheme_and_host(url: &str) -> Option<(String, String)> {
    let (scheme, _) = url.split_once(':')?;
    let scheme_is_valid = scheme.starts_with(|character: char| character.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+-.".contains(character));
    if !scheme_is_valid {
        return None;
    }
    let scheme = scheme.to_lowercase();
    if scheme != "http" && scheme != "https" {
        return Some((scheme, String::new()));
    }
    let uri: ureq::http::Uri = url.parse().ok()?;
    let host = uri.host().filter(|host| !host.is_empty())?;
    Some((scheme, host.to_lowercase()))
}

// The start of `text` up to `most` UTF-16 units, without splitting a character.
fn shorten(text: &str, most: usize) -> String {
    let mut units = 0;
    let mut kept = String::new();
    for character in text.chars() {
        units += character.len_utf16();
        if units > most {
            break;
        }
        kept.push(character);
    }
    kept
}

// The answer to one {"t":"http"} message, as {"t":"http","id":...} with `resp` or `err`.
pub async fn answer_http_request(message: &Value, allowed: &[String], fetch: &WebFetch) -> Value {
    let id = message["id"].clone();
    let failure = |text: String| json!({ "t": "http", "id": id, "err": text });
    let url = message["url"].as_str().unwrap_or_default();
    let Some((scheme, host)) = scheme_and_host(url) else {
        return failure("bad url".into());
    };
    if scheme != "http" && scheme != "https" {
        return failure("only http and https".into());
    }
    if !is_host_allowed(&host, allowed) {
        return failure(format!("{host} is not allowed"));
    }
    let method = message["method"].as_str().unwrap_or("GET").to_uppercase();
    if !HTTP_METHODS.contains(&method.as_str()) {
        return failure(format!("{method} is not supported"));
    }
    // GET and HEAD carry no body.
    let body = match method.as_str() {
        "GET" | "HEAD" => None,
        _ => message["body"].as_str().map(str::to_string),
    };
    match fetch(plain_request(&method, url, body)).await {
        Ok(reply) if !(200..300).contains(&reply.status) => failure(format!("HTTP {}", reply.status)),
        Ok(reply) => {
            let text = String::from_utf8_lossy(&reply.body);
            json!({ "t": "http", "id": id, "resp": shorten(&text, MOST_RESPONSE_UNITS) })
        }
        Err(error) if error.is_empty() => failure("fetch failed".into()),
        Err(error) => failure(error),
    }
}

// ============================================================================
// The proxy
// ============================================================================

pub struct HttpProxy {
    daemon: Arc<Daemon>,
    fetch: WebFetch,
}

impl HttpProxy {
    // Starts answering the watch's http requests over the real web.
    pub fn start(daemon: &Arc<Daemon>) -> Arc<HttpProxy> {
        HttpProxy::start_with_fetch(daemon, web_fetch())
    }

    pub fn start_with_fetch(daemon: &Arc<Daemon>, fetch: WebFetch) -> Arc<HttpProxy> {
        let proxy = Arc::new(HttpProxy {
            daemon: daemon.clone(),
            fetch,
        });
        daemon.runtime().spawn(proxy.clone().listen());
        proxy
    }

    // Runs for the daemon's life, answering each request on its own task so a slow host holds up no other.
    async fn listen(self: Arc<Self>) {
        let mut messages = self.daemon.watch_messages();
        loop {
            let message = match messages.recv().await {
                Ok(message) => message,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            };
            if message["t"] != "http" {
                continue;
            }
            let proxy = self.clone();
            self.daemon.runtime().spawn(async move {
                let reply = proxy.answer(&message).await;
                let _ = proxy.daemon.send(&reply).await;
            });
        }
    }

    // The reply to one request, refused while the user has the proxy turned off.
    pub async fn answer(&self, message: &Value) -> Value {
        let settings: HttpSettings = self.daemon.settings().view();
        if !settings.http_enabled {
            return json!({ "t": "http", "id": message["id"], "err": "disabled on the phone" });
        }
        answer_http_request(
            message,
            &parse_host_list(&settings.http_allowed_hosts),
            &self.fetch,
        )
        .await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::super::tests::{a_battletime_watch, pass, rig_with};
    use super::*;

    // A fake web that records each request and answers with `status` and `body`.
    fn fake_web(status: u16, body: &str, asked: Arc<Mutex<Vec<WebRequest>>>) -> WebFetch {
        let body = body.as_bytes().to_vec();
        Arc::new(move |outgoing: WebRequest| {
            asked.lock().unwrap().push(outgoing);
            let reply = WebReply {
                status,
                status_text: String::new(),
                body: body.clone(),
            };
            Box::pin(async move { Ok(reply) })
        })
    }

    fn allowed(hosts: &str) -> Vec<String> {
        parse_host_list(hosts)
    }

    #[test]
    fn a_host_or_any_of_its_subdomains_is_allowed() {
        let list = allowed("example.com, API.other.org");
        assert!(is_host_allowed("example.com", &list));
        assert!(is_host_allowed("www.Example.com", &list));
        assert!(is_host_allowed("api.other.org", &list));
        assert!(!is_host_allowed("badexample.com", &list));
        assert!(!is_host_allowed("other.org", &list));
        assert!(!is_host_allowed("anything", &[]));
    }

    #[test]
    fn a_host_list_splits_on_spaces_and_commas() {
        assert_eq!(
            allowed(" a.com,b.org\n c.net ,, "),
            vec!["a.com", "b.org", "c.net"]
        );
        assert!(allowed("").is_empty());
    }

    #[tokio::test]
    async fn bad_addresses_hosts_and_methods_are_refused_before_anything_is_sent() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let web = fake_web(200, "ok", asked.clone());
        let list = allowed("example.com");
        let answer = |message: Value| {
            let web = web.clone();
            let list = list.clone();
            async move { answer_http_request(&message, &list, &web).await["err"].clone() }
        };

        assert_eq!(
            answer(json!({ "t": "http", "id": "1", "url": "nothing" })).await,
            "bad url"
        );
        assert_eq!(
            answer(json!({ "t": "http", "id": "1", "url": "ftp://example.com/a" })).await,
            "only http and https"
        );
        assert_eq!(
            answer(json!({ "t": "http", "id": "1", "url": "https://Evil.com/a" })).await,
            "evil.com is not allowed"
        );
        assert_eq!(
            answer(json!({ "t": "http", "id": "1", "url": "https://example.com", "method": "brew" })).await,
            "BREW is not supported"
        );
        assert!(asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_allowed_request_is_fetched_and_its_answer_cut_short() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let long = "é".repeat(MOST_RESPONSE_UNITS + 10);
        let web = fake_web(200, &long, asked.clone());

        let reply = answer_http_request(
            &json!({ "t": "http", "id": "7", "url": "https://api.example.com/x", "method": "post", "body": "hi" }),
            &allowed("example.com"),
            &web,
        )
        .await;

        assert_eq!(reply["id"], "7");
        assert_eq!(
            reply["resp"].as_str().unwrap().chars().count(),
            MOST_RESPONSE_UNITS
        );
        let asked = asked.lock().unwrap();
        assert_eq!(asked[0].method, "POST");
        assert_eq!(asked[0].url, "https://api.example.com/x");
        assert_eq!(asked[0].body.as_deref(), Some("hi"));
    }

    #[tokio::test]
    async fn a_get_carries_no_body_and_a_failed_status_is_an_error() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let web = fake_web(404, "missing", asked.clone());

        let reply = answer_http_request(
            &json!({ "t": "http", "id": "2", "url": "http://example.com/", "body": "ignored" }),
            &allowed("example.com"),
            &web,
        )
        .await;

        assert_eq!(reply, json!({ "t": "http", "id": "2", "err": "HTTP 404" }));
        assert_eq!(asked.lock().unwrap()[0].body, None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_watch_hears_back_only_while_the_user_allows_it() {
        let rig = rig_with(
            json!({ "httpEnabled": false, "httpAllowedHosts": "example.com" }),
            a_battletime_watch,
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        let _proxy = HttpProxy::start_with_fetch(&rig.daemon, fake_web(200, "hello", asked.clone()));
        rig.daemon.connect("AA", "BattleTime").await.unwrap();
        pass(Duration::from_secs(3)).await;

        let request = "{\"t\":\"http\",\"id\":\"5\",\"url\":\"https://example.com/\"}\n";
        rig.daemon.on_link_event("line", json!(request));
        pass(Duration::from_secs(1)).await;
        assert_eq!(
            rig.link.sent_of("http"),
            vec![json!({ "t": "http", "id": "5", "err": "disabled on the phone" })]
        );

        rig.daemon
            .update_settings(&json!({ "httpEnabled": true }))
            .unwrap();
        rig.daemon.on_link_event("line", json!(request));
        pass(Duration::from_secs(1)).await;
        assert_eq!(
            rig.link.sent_of("http")[1],
            json!({ "t": "http", "id": "5", "resp": "hello" })
        );
        assert_eq!(asked.lock().unwrap().len(), 1);
    }
}
