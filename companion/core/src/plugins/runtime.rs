// One plugin's logic, running in QuickJS on a thread of its own.
//
// The script sees `neotime` (events, messages to the watch and the settings page, settings), timers,
// `fetch` and `console`. Each call out of the script goes through a few host functions that check it
// against the manifest; everything else is the JavaScript in PRELUDE.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rquickjs::{Context, Ctx, Function, Object, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::manifest::Manifest;

// ============================================================================
// Limits
// ============================================================================

// The heap one plugin's script may use.
const MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

// The stack one plugin's script may use, which stops runaway recursion.
const STACK_LIMIT_BYTES: usize = 512 * 1024;

// How long one event may run before the script is stopped, which stops an endless loop.
const EVENT_TIME_LIMIT: Duration = Duration::from_secs(2);

// The most requests one plugin may have waiting at once.
const MOST_FETCHES_AT_ONCE: usize = 8;

// ============================================================================
// What the host offers
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: String,
    #[serde(default = "get_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Map<String, Value>,
    #[serde(default)]
    pub body: Option<String>,
}

fn get_method() -> String {
    "GET".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchResponse {
    pub status: u16,
    pub body: String,
}

// What a plugin's script reaches outside itself. The host decides how each is carried out.
pub trait PluginServices: Send + Sync + 'static {
    // A message for the watch, already checked against the manifest.
    fn send_to_watch(&self, plugin: &str, message: Value);
    fn send_to_ui(&self, plugin: &str, message: Value);
    fn log(&self, plugin: &str, line: &str);
    fn save_settings(&self, plugin: &str, settings: &Map<String, Value>);
    // Runs on a thread of its own, so it may block.
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, String>;
}

// What happens to a plugin, in the order it happens.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    // The `d` of an app message on the plugin's channel.
    Message(Value),
    Connected,
    Disconnected,
    // A message from the plugin's settings page.
    Ui(Value),
    FetchDone {
        id: u64,
        result: Result<FetchResponse, String>,
    },
    Stop,
}

// ============================================================================
// The script's side
// ============================================================================

const PRELUDE: &str = r#"
(() => {
  const host = globalThis.__host;
  delete globalThis.__host;
  const handlers = { message: [], connect: [], disconnect: [], ui: [] };
  const timers = new Map();
  const fetches = new Map();
  let nextTimer = 1;
  let nextFetch = 1;
  const describe = (error) => (error && error.stack ? `${error}\n${error.stack}` : String(error));
  const report = (error) => host.log(`error: ${describe(error)}`);
  const run = (handler, argument) => {
    try {
      const result = handler(argument);
      if (result && typeof result.then === "function") result.then(undefined, report);
    } catch (error) {
      report(error);
    }
  };
  const text = (values) => values.map((value) => (typeof value === "string" ? value : JSON.stringify(value))).join(" ");
  globalThis.console = Object.freeze({
    log: (...values) => host.log(text(values)),
    warn: (...values) => host.log(`warning: ${text(values)}`),
    error: (...values) => host.log(`error: ${text(values)}`),
  });
  const startTimer = (handler, delay, repeat) => {
    const id = nextTimer++;
    const milliseconds = Math.max(repeat ? 1000 : 0, Number(delay) || 0);
    timers.set(id, { handler, milliseconds, repeat });
    host.timer(id, milliseconds);
    return id;
  };
  globalThis.setTimeout = (handler, delay) => startTimer(handler, delay, false);
  globalThis.setInterval = (handler, delay) => startTimer(handler, delay, true);
  globalThis.clearTimeout = globalThis.clearInterval = (id) => {
    if (timers.delete(id)) host.cancel(id);
  };
  globalThis.fetch = (url, options = {}) =>
    new Promise((resolve, reject) => {
      const id = nextFetch++;
      const refusal = host.fetch(id, JSON.stringify({
        url: String(url),
        method: String(options.method || "GET").toUpperCase(),
        headers: options.headers || {},
        body: options.body == null ? null : String(options.body),
      }));
      if (refusal) reject(new Error(refusal));
      else fetches.set(id, { resolve, reject });
    });
  globalThis.neotime = Object.freeze({
    on(kind, handler) {
      if (!handlers[kind]) throw new Error(`neotime.on has no "${kind}" event`);
      handlers[kind].push(handler);
    },
    send(message) {
      const refusal = host.send(JSON.stringify(message));
      if (refusal) throw new Error(refusal);
    },
    ui: Object.freeze({ send: (message) => host.ui(JSON.stringify(message)) }),
    settings: Object.freeze({
      get(key) {
        const value = host.get(String(key));
        return value === "" ? undefined : JSON.parse(value);
      },
      set(key, value) {
        host.set(String(key), JSON.stringify(value === undefined ? null : value));
      },
    }),
  });
  globalThis.__dispatch = (kind, json) => {
    const data = json === "" ? undefined : JSON.parse(json);
    if (kind === "timer") {
      const timer = timers.get(data);
      if (!timer) return;
      if (timer.repeat) host.timer(data, timer.milliseconds);
      else timers.delete(data);
      run(timer.handler);
      return;
    }
    if (kind === "fetch") {
      const waiting = fetches.get(data.id);
      if (!waiting) return;
      fetches.delete(data.id);
      if (data.error) {
        waiting.reject(new Error(data.error));
        return;
      }
      const body = data.body;
      waiting.resolve(Object.freeze({
        status: data.status,
        ok: data.status >= 200 && data.status < 300,
        text: () => Promise.resolve(body),
        json: () => Promise.resolve().then(() => JSON.parse(body)),
      }));
      return;
    }
    for (const handler of handlers[kind] || []) run(handler, data);
  };
})();
"#;

// ============================================================================
// Checks on what the script asks for
// ============================================================================

// Why the script may not send this message to the watch, or None when it may.
pub fn refuse_message(manifest: &Manifest, message: &Value) -> Option<String> {
    let Some(message_type) = message.get("t").and_then(Value::as_str) else {
        return Some("A message for the watch needs a \"t\" naming its type.".into());
    };
    if manifest.may_send(message_type) {
        None
    } else {
        Some(format!(
            "This plugin may not send \"{message_type}\" messages; add it to permissions.messages."
        ))
    }
}

// Why the script may not fetch this address, or None when it may.
pub fn refuse_fetch(manifest: &Manifest, url: &str) -> Option<String> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Some(format!("{url} is not an https address."));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Some(format!("{url} carries a user name, which plugins may not send."));
    }
    let host = authority.split(':').next().unwrap_or("");
    if manifest.may_fetch(host) {
        None
    } else {
        Some(format!(
            "This plugin may not fetch from {host}; add it to permissions.hosts."
        ))
    }
}

// ============================================================================
// A running plugin
// ============================================================================

pub struct RunningPlugin {
    sender: Sender<Event>,
    thread: Option<JoinHandle<()>>,
}

impl RunningPlugin {
    // Loads the script and runs its top level; an error there stops the plugin before it starts.
    pub fn start(
        manifest: Manifest,
        source: String,
        settings: Map<String, Value>,
        services: Arc<dyn PluginServices>,
    ) -> Result<RunningPlugin, String> {
        let (sender, receiver) = mpsc::channel();
        let (started_sender, started) = mpsc::channel();
        let loop_sender = sender.clone();
        let thread = std::thread::Builder::new()
            .name(format!("plugin {}", manifest.id))
            .spawn(move || {
                let mut plugin = match Script::load(manifest, &source, settings, services, loop_sender) {
                    Ok(plugin) => plugin,
                    Err(error) => {
                        let _ = started_sender.send(Err(error));
                        return;
                    }
                };
                let _ = started_sender.send(Ok(()));
                plugin.run(receiver);
            })
            .map_err(|error| error.to_string())?;
        match started.recv() {
            Ok(Ok(())) => Ok(RunningPlugin {
                sender,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err("The plugin's thread stopped while starting.".into()),
        }
    }

    pub fn send(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let _ = self.sender.send(Event::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RunningPlugin {
    fn drop(&mut self) {
        self.shut_down();
    }
}

// ============================================================================
// The script's thread
// ============================================================================

#[derive(Default)]
struct Requests {
    timers: Vec<(u64, u64)>,
    cancelled: Vec<u64>,
    fetches: Vec<(u64, FetchRequest)>,
    settings_changed: bool,
}

struct Script {
    id: String,
    runtime: Runtime,
    context: Context,
    services: Arc<dyn PluginServices>,
    requests: Rc<RefCell<Requests>>,
    settings: Rc<RefCell<Map<String, Value>>>,
    deadline: Rc<Cell<Option<Instant>>>,
    sender: Sender<Event>,
    fetches_waiting: Rc<Cell<usize>>,
    // Timers by when they are due, and those cleared before they fired.
    due: RefCell<BinaryHeap<Reverse<(Instant, u64)>>>,
    cancelled: RefCell<HashSet<u64>>,
}

impl Script {
    fn load(
        manifest: Manifest,
        source: &str,
        settings: Map<String, Value>,
        services: Arc<dyn PluginServices>,
        sender: Sender<Event>,
    ) -> Result<Script, String> {
        let runtime = Runtime::new().map_err(|error| error.to_string())?;
        runtime.set_memory_limit(MEMORY_LIMIT_BYTES);
        runtime.set_max_stack_size(STACK_LIMIT_BYTES);
        let deadline = Rc::new(Cell::new(None::<Instant>));
        let watched = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            watched.get().is_some_and(|until| Instant::now() > until)
        })));
        let context = Context::full(&runtime).map_err(|error| error.to_string())?;

        let script = Script {
            id: manifest.id.clone(),
            runtime,
            context,
            services,
            requests: Rc::default(),
            settings: Rc::new(RefCell::new(settings)),
            deadline,
            sender,
            fetches_waiting: Rc::default(),
            due: RefCell::default(),
            cancelled: RefCell::default(),
        };
        let manifest = Rc::new(manifest);
        script.deadline.set(Some(Instant::now() + EVENT_TIME_LIMIT));
        let loaded = script.context.with(|ctx| -> Result<(), String> {
            script
                .install_host(&ctx, manifest)
                .map_err(|error| error.to_string())?;
            ctx.eval::<(), _>(PRELUDE)
                .map_err(|error| exception_text(&ctx, error))?;
            ctx.eval::<(), _>(source)
                .map_err(|error| exception_text(&ctx, error))
        });
        script.deadline.set(None);
        loaded.map_err(|error| format!("The plugin's script failed to start: {error}"))?;
        script.run_jobs();
        script.apply_requests();
        Ok(script)
    }

    // The host functions the prelude calls, each returning a refusal or "" for done.
    fn install_host<'js>(&self, ctx: &Ctx<'js>, manifest: Rc<Manifest>) -> rquickjs::Result<()> {
        let host = Object::new(ctx.clone())?;

        let (services, id, checked) = (self.services.clone(), self.id.clone(), manifest.clone());
        host.set(
            "send",
            Function::new(ctx.clone(), move |text: String| -> String {
                let Ok(message) = serde_json::from_str::<Value>(&text) else {
                    return "The message is not JSON.".into();
                };
                if let Some(refusal) = refuse_message(&checked, &message) {
                    return refusal;
                }
                services.send_to_watch(&id, message);
                String::new()
            })?,
        )?;

        let (services, id) = (self.services.clone(), self.id.clone());
        host.set(
            "ui",
            Function::new(ctx.clone(), move |text: String| {
                services.send_to_ui(&id, serde_json::from_str(&text).unwrap_or(Value::Null));
            })?,
        )?;

        let (services, id) = (self.services.clone(), self.id.clone());
        host.set(
            "log",
            Function::new(ctx.clone(), move |line: String| services.log(&id, &line))?,
        )?;

        let settings = self.settings.clone();
        host.set(
            "get",
            Function::new(ctx.clone(), move |key: String| -> String {
                settings
                    .borrow()
                    .get(&key)
                    .map(Value::to_string)
                    .unwrap_or_default()
            })?,
        )?;

        let (settings, requests) = (self.settings.clone(), self.requests.clone());
        host.set(
            "set",
            Function::new(ctx.clone(), move |key: String, text: String| {
                settings
                    .borrow_mut()
                    .insert(key, serde_json::from_str(&text).unwrap_or(Value::Null));
                requests.borrow_mut().settings_changed = true;
            })?,
        )?;

        let requests = self.requests.clone();
        host.set(
            "timer",
            Function::new(ctx.clone(), move |id: f64, milliseconds: f64| {
                requests
                    .borrow_mut()
                    .timers
                    .push((id as u64, milliseconds.max(0.0) as u64));
            })?,
        )?;

        let requests = self.requests.clone();
        host.set(
            "cancel",
            Function::new(ctx.clone(), move |id: f64| {
                requests.borrow_mut().cancelled.push(id as u64)
            })?,
        )?;

        let (requests, waiting) = (self.requests.clone(), self.fetches_waiting.clone());
        host.set(
            "fetch",
            Function::new(ctx.clone(), move |id: f64, text: String| -> String {
                let Ok(request) = serde_json::from_str::<FetchRequest>(&text) else {
                    return "The request is not valid.".into();
                };
                if let Some(refusal) = refuse_fetch(&manifest, &request.url) {
                    return refusal;
                }
                if waiting.get() >= MOST_FETCHES_AT_ONCE {
                    return format!("A plugin may have at most {MOST_FETCHES_AT_ONCE} requests waiting.");
                }
                waiting.set(waiting.get() + 1);
                requests.borrow_mut().fetches.push((id as u64, request));
                String::new()
            })?,
        )?;

        ctx.globals().set("__host", host)
    }

    fn run(&mut self, receiver: Receiver<Event>) {
        loop {
            let next_due = self.due.borrow().peek().map(|Reverse((when, _))| *when);
            let event = match next_due {
                Some(when) => match receiver.recv_timeout(when.saturating_duration_since(Instant::now())) {
                    Ok(event) => Some(event),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                },
                None => match receiver.recv() {
                    Ok(event) => Some(event),
                    Err(_) => return,
                },
            };
            match event {
                Some(Event::Stop) => return,
                Some(Event::Message(data)) => self.dispatch("message", &data),
                Some(Event::Connected) => self.dispatch("connect", &Value::Null),
                Some(Event::Disconnected) => self.dispatch("disconnect", &Value::Null),
                Some(Event::Ui(data)) => self.dispatch("ui", &data),
                Some(Event::FetchDone { id, result }) => {
                    self.fetches_waiting
                        .set(self.fetches_waiting.get().saturating_sub(1));
                    let data = match result {
                        Ok(response) => json!({ "id": id, "status": response.status, "body": response.body }),
                        Err(error) => json!({ "id": id, "error": error }),
                    };
                    self.dispatch("fetch", &data);
                }
                None => {}
            }
            self.apply_requests();
            let now = Instant::now();
            loop {
                let fired = self.due.borrow().peek().copied();
                let Some(Reverse((when, id))) = fired else { break };
                if when > now {
                    break;
                }
                self.due.borrow_mut().pop();
                if !self.cancelled.borrow_mut().remove(&id) {
                    self.dispatch("timer", &json!(id));
                    self.apply_requests();
                }
            }
        }
    }

    // Hands one event to the script, then lets any promises it started settle.
    fn dispatch(&self, kind: &str, data: &Value) {
        let text = if data.is_null() {
            String::new()
        } else {
            data.to_string()
        };
        self.deadline.set(Some(Instant::now() + EVENT_TIME_LIMIT));
        let outcome = self.context.with(|ctx| -> Result<(), String> {
            let dispatch: Function = ctx
                .globals()
                .get("__dispatch")
                .map_err(|error| error.to_string())?;
            dispatch
                .call::<_, ()>((kind, text))
                .map_err(|error| exception_text(&ctx, error))
        });
        if let Err(error) = outcome {
            self.services.log(&self.id, &format!("error: {error}"));
        }
        self.run_jobs();
        self.deadline.set(None);
    }

    fn run_jobs(&self) {
        loop {
            match self.runtime.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(job) => {
                    let text = job.0.with(|ctx| exception_text(&ctx, rquickjs::Error::Exception));
                    self.services.log(&self.id, &format!("error: {text}"));
                }
            }
        }
    }

    // Saves changed settings, starts requested fetches and schedules or clears timers.
    fn apply_requests(&self) {
        let requests = std::mem::take(&mut *self.requests.borrow_mut());
        if requests.settings_changed {
            self.services.save_settings(&self.id, &self.settings.borrow());
        }
        for (id, request) in requests.fetches {
            let services = self.services.clone();
            let sender = self.sender.clone();
            std::thread::spawn(move || {
                let result = services.fetch(&request);
                let _ = sender.send(Event::FetchDone { id, result });
            });
        }
        for id in requests.cancelled {
            self.cancelled.borrow_mut().insert(id);
        }
        for (id, milliseconds) in requests.timers {
            self.cancelled.borrow_mut().remove(&id);
            self.due.borrow_mut().push(Reverse((
                Instant::now() + Duration::from_millis(milliseconds),
                id,
            )));
        }
    }
}

// The script's exception as text, with its stack when it has one.
fn exception_text(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    if !matches!(error, rquickjs::Error::Exception) {
        return error.to_string();
    }
    let thrown = ctx.catch();
    if let Some(exception) = thrown.as_exception() {
        let message = exception.message().unwrap_or_default();
        return match exception.stack() {
            Some(stack) if !stack.is_empty() => format!("{message}\n{}", stack.trim_end()),
            _ => message,
        };
    }
    thrown
        .as_string()
        .and_then(|text| text.to_string().ok())
        .unwrap_or_else(|| "the script threw something that is not an error".into())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    // Records what a plugin asks of its host, and answers fetches from a fixed table.
    #[derive(Default)]
    pub struct Recorder {
        pub watch: Mutex<Vec<Value>>,
        pub ui: Mutex<Vec<Value>>,
        pub log: Mutex<Vec<String>>,
        pub settings: Mutex<Vec<Map<String, Value>>>,
        pub fetched: Mutex<Vec<FetchRequest>>,
    }

    impl PluginServices for Recorder {
        fn send_to_watch(&self, _plugin: &str, message: Value) {
            self.watch.lock().unwrap().push(message);
        }
        fn send_to_ui(&self, _plugin: &str, message: Value) {
            self.ui.lock().unwrap().push(message);
        }
        fn log(&self, _plugin: &str, line: &str) {
            self.log.lock().unwrap().push(line.to_string());
        }
        fn save_settings(&self, _plugin: &str, settings: &Map<String, Value>) {
            self.settings.lock().unwrap().push(settings.clone());
        }
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, String> {
            self.fetched.lock().unwrap().push(request.clone());
            if request.url.contains("fail") {
                return Err("no route".into());
            }
            Ok(FetchResponse {
                status: 200,
                body: r#"{"temp":290}"#.into(),
            })
        }
    }

    pub fn manifest(messages: &[&str], hosts: &[&str]) -> Manifest {
        Manifest::parse(
            &json!({
                "id": "demo", "name": "Demo", "version": "1",
                "permissions": { "messages": messages, "hosts": hosts }
            })
            .to_string(),
        )
        .unwrap()
    }

    fn start(source: &str, messages: &[&str], hosts: &[&str]) -> (RunningPlugin, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let plugin = RunningPlugin::start(
            manifest(messages, hosts),
            source.into(),
            Map::new(),
            recorder.clone(),
        )
        .unwrap();
        (plugin, recorder)
    }

    // Waits up to five seconds for `ready` to hold.
    pub fn wait_for(ready: impl Fn() -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !ready() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ready(), "timed out waiting");
    }

    #[test]
    fn a_watch_message_gets_an_answer_the_manifest_allows() {
        let (plugin, recorder) = start(
            "neotime.on('message', (d) => neotime.send({ t: 'vibrate', n: d.times }));",
            &["vibrate"],
            &[],
        );
        plugin.send(Event::Message(json!({ "times": 2 })));
        wait_for(|| recorder.watch.lock().unwrap().len() == 1);
        assert_eq!(
            recorder.watch.lock().unwrap()[0],
            json!({ "t": "vibrate", "n": 2 })
        );
    }

    #[test]
    fn a_message_the_manifest_does_not_allow_throws_in_the_script() {
        let (plugin, recorder) = start(
            "neotime.on('connect', () => { try { neotime.send({ t: 'vibrate' }); } catch (e) { console.log(e.message); } });",
            &[],
            &[],
        );
        plugin.send(Event::Connected);
        wait_for(|| !recorder.log.lock().unwrap().is_empty());
        assert!(recorder.log.lock().unwrap()[0].contains("may not send \"vibrate\""));
        assert!(recorder.watch.lock().unwrap().is_empty());
    }

    #[test]
    fn settings_read_back_and_are_saved_once_per_event() {
        let (plugin, recorder) = start(
            "neotime.on('ui', (m) => { neotime.settings.set('city', m.city); neotime.ui.send({ city: neotime.settings.get('city'), none: neotime.settings.get('none') === undefined }); });",
            &[],
            &[],
        );
        plugin.send(Event::Ui(json!({ "city": "Kingston" })));
        wait_for(|| recorder.ui.lock().unwrap().len() == 1);
        assert_eq!(
            recorder.ui.lock().unwrap()[0],
            json!({ "city": "Kingston", "none": true })
        );
        assert_eq!(
            recorder.settings.lock().unwrap().as_slice(),
            [json!({"city": "Kingston"}).as_object().unwrap().clone()]
        );
    }

    #[test]
    fn timers_fire_repeat_and_stop_when_cleared() {
        let (plugin, recorder) = start(
            "let n = 0; setTimeout(() => console.log('once'), 10); const t = setInterval(() => { n++; console.log('tick ' + n); if (n === 2) clearInterval(t); }, 1);",
            &[],
            &[],
        );
        wait_for(|| recorder.log.lock().unwrap().len() == 3);
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(
            recorder.log.lock().unwrap().as_slice(),
            ["once", "tick 1", "tick 2"]
        );
        plugin.stop();
    }

    #[test]
    fn fetch_reaches_only_the_hosts_the_manifest_names() {
        let (plugin, recorder) = start(
            r#"
            neotime.on('connect', async () => {
              const reply = await fetch('https://api.example.com/now?x=1', { method: 'post', body: 'b' });
              console.log(reply.ok, (await reply.json()).temp);
              await fetch('https://other.example.com/').catch((e) => console.log(e.message));
              await fetch('http://api.example.com/').catch((e) => console.log(e.message));
              await fetch('https://api.example.com/fail').catch((e) => console.log(e.message));
            });"#,
            &[],
            &["api.example.com"],
        );
        plugin.send(Event::Connected);
        wait_for(|| recorder.log.lock().unwrap().len() == 4);
        let log = recorder.log.lock().unwrap().clone();
        assert_eq!(log[0], "true 290");
        assert!(log[1].contains("may not fetch from other.example.com"));
        assert!(log[2].contains("not an https address"));
        assert_eq!(log[3], "no route");
        let fetched = recorder.fetched.lock().unwrap();
        assert_eq!(fetched[0].method, "POST");
        assert_eq!(fetched[0].body.as_deref(), Some("b"));
    }

    #[test]
    fn errors_in_handlers_are_logged_and_the_plugin_keeps_running() {
        let (plugin, recorder) = start(
            "neotime.on('message', () => { throw new Error('boom'); }); neotime.on('message', () => console.log('still here'));",
            &[],
            &[],
        );
        plugin.send(Event::Message(json!({})));
        wait_for(|| recorder.log.lock().unwrap().len() == 2);
        let log = recorder.log.lock().unwrap().clone();
        assert!(log[0].starts_with("error: Error: boom"), "{log:?}");
        assert_eq!(log[1], "still here");
    }

    #[test]
    fn an_endless_loop_is_stopped_and_the_next_event_still_runs() {
        let (plugin, recorder) = start(
            "neotime.on('message', (d) => { if (d.spin) { for (;;) {} } console.log('next'); });",
            &[],
            &[],
        );
        plugin.send(Event::Message(json!({ "spin": true })));
        plugin.send(Event::Message(json!({})));
        let until = Instant::now() + Duration::from_secs(5);
        while recorder.log.lock().unwrap().len() < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        let log = recorder.log.lock().unwrap().clone();
        assert!(log[0].contains("interrupted"), "{log:?}");
        assert_eq!(log[1], "next");
    }

    #[test]
    fn a_script_that_fails_to_load_does_not_start() {
        let recorder = Arc::new(Recorder::default());
        let failed = RunningPlugin::start(
            manifest(&[], &[]),
            "this is not javascript".into(),
            Map::new(),
            recorder,
        );
        assert!(failed.err().unwrap().contains("failed to start"));
    }

    #[test]
    fn an_unknown_event_name_is_an_error_at_load() {
        let recorder = Arc::new(Recorder::default());
        let failed = RunningPlugin::start(
            manifest(&[], &[]),
            "neotime.on('nope', () => {});".into(),
            Map::new(),
            recorder,
        );
        assert!(failed.err().unwrap().contains("no \"nope\" event"));
    }

    #[test]
    fn refusals_name_what_is_missing() {
        let manifest = manifest(&["weather"], &["api.example.com"]);
        assert_eq!(refuse_message(&manifest, &json!({ "t": "weather" })), None);
        assert!(refuse_message(&manifest, &json!({}))
            .unwrap()
            .contains("needs a \"t\""));
        assert_eq!(refuse_fetch(&manifest, "https://api.example.com:443/x"), None);
        assert!(refuse_fetch(&manifest, "https://user@api.example.com/")
            .unwrap()
            .contains("user name"));
    }
}
