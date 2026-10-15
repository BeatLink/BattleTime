// Carries out a plugin's fetch, after the runtime has checked the address against its manifest.

use std::time::Duration;

use super::runtime::{FetchRequest, FetchResponse};

// How long a request may take, start to finish.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

// The most of a reply a plugin gets, so a huge page cannot fill its memory.
const MOST_REPLY_BYTES: u64 = 1024 * 1024;

pub fn fetch(request: &FetchRequest) -> Result<FetchResponse, String> {
    // Redirects are handed back rather than followed, since one could lead to a host the manifest does not name.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let headers: Vec<(String, String)> = request
        .headers
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect();

    let reply = match request.method.as_str() {
        "GET" | "DELETE" | "HEAD" => {
            let mut builder = match request.method.as_str() {
                "GET" => agent.get(&request.url),
                "DELETE" => agent.delete(&request.url),
                _ => agent.head(&request.url),
            };
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
            builder.call()
        }
        "POST" | "PUT" | "PATCH" => {
            let mut builder = match request.method.as_str() {
                "POST" => agent.post(&request.url),
                "PUT" => agent.put(&request.url),
                _ => agent.patch(&request.url),
            };
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
            builder.send(request.body.clone().unwrap_or_default())
        }
        other => return Err(format!("Plugins cannot send {other} requests.")),
    };
    let mut reply = reply.map_err(|error| error.to_string())?;
    let status = reply.status().as_u16();
    let body = reply
        .body_mut()
        .with_config()
        .limit(MOST_REPLY_BYTES)
        .read_to_string()
        .map_err(|error| error.to_string())?;
    Ok(FetchResponse { status, body })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_method_is_refused_before_anything_is_sent() {
        let request = FetchRequest {
            url: "https://example.com".into(),
            method: "TRACE".into(),
            headers: Default::default(),
            body: None,
        };
        assert_eq!(fetch(&request), Err("Plugins cannot send TRACE requests.".into()));
    }

    #[test]
    fn an_unreachable_host_is_an_error_not_a_hang() {
        let request = FetchRequest {
            url: "https://battletime-plugin-test.invalid/".into(),
            method: "GET".into(),
            headers: Default::default(),
            body: None,
        };
        assert!(fetch(&request).is_err());
    }
}
