// Matches the watch's answers to the daemon's requests for them, oldest first, as src/protocol/requests.ts does.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;

// How long the watch has to answer before a request fails.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

struct Waiter {
    number: u64,
    reply_type: String,
    answer: oneshot::Sender<Result<Value, String>>,
}

#[derive(Default)]
pub struct ReplyRouter {
    waiting: Mutex<Vec<Waiter>>,
    next: Mutex<u64>,
}

pub struct PendingReply {
    number: u64,
    answer: oneshot::Receiver<Result<Value, String>>,
}

impl ReplyRouter {
    // Starts waiting for an answer of `reply_type`; send the request after this, so a quick answer is not missed.
    pub fn expect(&self, reply_type: &str) -> PendingReply {
        let (sender, receiver) = oneshot::channel();
        let number = {
            let mut next = self.next.lock().unwrap();
            *next += 1;
            *next
        };
        self.waiting.lock().unwrap().push(Waiter {
            number,
            reply_type: reply_type.to_string(),
            answer: sender,
        });
        PendingReply {
            number,
            answer: receiver,
        }
    }

    pub async fn wait(&self, pending: PendingReply, limit: Duration) -> Result<Value, String> {
        let PendingReply { number, answer } = pending;
        let outcome = tokio::time::timeout(limit, answer).await;
        self.waiting
            .lock()
            .unwrap()
            .retain(|waiter| waiter.number != number);
        match outcome {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err("The watch disconnected.".into()),
            Err(_) => Err("The watch did not reply.".into()),
        }
    }

    // Gives up on a request whose message never reached the watch.
    pub fn cancel(&self, pending: &PendingReply) {
        self.waiting
            .lock()
            .unwrap()
            .retain(|waiter| waiter.number != pending.number);
    }

    // Offers a message from the watch, and says whether a request took it.
    pub fn push(&self, message: &Value) -> bool {
        let mut waiting = self.waiting.lock().unwrap();
        let kind = message["t"].as_str().unwrap_or_default();
        let found = if kind == "error" {
            // The watch reports a failed message without saying which, so it fails the oldest request.
            (!waiting.is_empty()).then_some(0)
        } else {
            waiting.iter().position(|waiter| waiter.reply_type == kind)
        };
        let Some(found) = found else {
            return false;
        };
        let waiter = waiting.remove(found);
        let answer = if kind == "error" {
            Err(last_line(message["msg"].as_str().unwrap_or_default()))
        } else {
            Ok(message.clone())
        };
        let _ = waiter.answer.send(answer);
        true
    }

    // Fails every request still waiting, as the link has gone.
    pub fn reset(&self) {
        for waiter in self.waiting.lock().unwrap().drain(..) {
            let _ = waiter.answer.send(Err("The watch disconnected.".into()));
        }
    }
}

// A multi-line error ends with the line that says what went wrong.
fn last_line(text: &str) -> String {
    text.trim()
        .lines()
        .last()
        .filter(|line| !line.is_empty())
        .unwrap_or("The watch reported an error.")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn an_answer_goes_to_the_request_waiting_for_its_type() {
        let router = ReplyRouter::default();
        let status = router.expect("status");
        let settings = router.expect("settings");

        assert!(router.push(&json!({ "t": "settings", "brightness": 2 })));
        assert!(router.push(&json!({ "t": "status", "bat": 80 })));
        assert!(!router.push(&json!({ "t": "status", "bat": 81 })));

        assert_eq!(router.wait(status, REPLY_TIMEOUT).await.unwrap()["bat"], 80);
        assert_eq!(
            router.wait(settings, REPLY_TIMEOUT).await.unwrap()["brightness"],
            2
        );
    }

    #[tokio::test]
    async fn an_error_fails_the_oldest_request_with_its_last_line() {
        let router = ReplyRouter::default();
        let first = router.expect("status");
        let second = router.expect("status");

        assert!(router.push(&json!({ "t": "error", "msg": "Traceback\nValueError: bad\n" })));

        assert_eq!(
            router.wait(first, REPLY_TIMEOUT).await,
            Err("ValueError: bad".into())
        );
        router.reset();
        assert_eq!(
            router.wait(second, REPLY_TIMEOUT).await,
            Err("The watch disconnected.".into())
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_the_watch_never_answers_times_out_and_is_forgotten() {
        let router = ReplyRouter::default();
        let pending = router.expect("status");

        assert_eq!(
            router.wait(pending, REPLY_TIMEOUT).await,
            Err("The watch did not reply.".into())
        );
        assert!(!router.push(&json!({ "t": "status" })));
    }

    #[test]
    fn an_error_with_no_request_waiting_is_left_for_others() {
        let router = ReplyRouter::default();
        let pending = router.expect("status");
        router.cancel(&pending);
        assert!(!router.push(&json!({ "t": "error", "msg": "" })));
    }
}
