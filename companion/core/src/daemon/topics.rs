// The topic router: hands each app message from the watch to the background jobs that hear its topic.
//
// A job subscribes to a topic, such as "sos", and reads its messages from the receiver it gets back.
// The daemon also hands every message to the plugins, which hear the topic named by their id.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::mpsc;

use crate::protocol::app_message::AppMessage;

#[derive(Default)]
pub struct TopicRouter {
    subscribers: Mutex<HashMap<String, Vec<mpsc::UnboundedSender<AppMessage>>>>,
}

impl TopicRouter {
    // A receiver of every message on this topic from now on.
    pub fn subscribe(&self, topic: &str) -> mpsc::UnboundedReceiver<AppMessage> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.subscribers
            .lock()
            .unwrap()
            .entry(topic.to_string())
            .or_default()
            .push(sender);
        receiver
    }

    // Hands the message to its topic's subscribers, forgets those that stopped listening, and says whether any heard it.
    pub fn deliver(&self, message: &AppMessage) -> bool {
        let mut subscribers = self.subscribers.lock().unwrap();
        let Some(listening) = subscribers.get_mut(&message.topic) else {
            return false;
        };
        listening.retain(|sender| sender.send(message.clone()).is_ok());
        let heard = !listening.is_empty();
        if !heard {
            subscribers.remove(&message.topic);
        }
        heard
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_reaches_only_its_topics_subscribers() {
        let router = TopicRouter::default();
        let mut sos = router.subscribe("sos");
        let mut also_sos = router.subscribe("sos");
        let mut remote = router.subscribe("remote");
        let message = AppMessage::new("sos").with_number("on", 1);
        assert!(router.deliver(&message));
        assert_eq!(sos.try_recv().unwrap(), message);
        assert_eq!(also_sos.try_recv().unwrap(), message);
        assert!(remote.try_recv().is_err());
        assert!(!router.deliver(&AppMessage::new("nobody")));
    }

    #[test]
    fn a_subscriber_that_stopped_listening_is_forgotten() {
        let router = TopicRouter::default();
        drop(router.subscribe("sos"));
        assert!(!router.deliver(&AppMessage::new("sos")));
        assert!(router.subscribers.lock().unwrap().is_empty());
    }
}
