// Sends current conditions from Open-Meteo to the watch when it connects and every half hour, as the window once did.
//
// It mirrors src/services/weather.ts and the weather effect in src/state/watch-provider.tsx.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio::time::Instant;

use super::device::Location;
use super::http_proxy::{plain_request, web_fetch, WebFetch};
use super::Daemon;

// ============================================================================
// Timing
// ============================================================================

// How often the weather is sent while a watch is connected.
pub const WEATHER_INTERVAL: Duration = Duration::from_secs(30 * 60);

// ============================================================================
// Settings
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WeatherSettings {
    pub forward_weather: bool,
    pub weather_place: Option<Location>,
    pub weather_from_device: bool,
}

// ============================================================================
// Codes and units
// ============================================================================

// Open-Meteo reports conditions as WMO weather codes, each label covering every code up to its own.
const CONDITIONS: [(f64, &str); 11] = [
    (0.0, "Clear"),
    (1.0, "Mostly clear"),
    (2.0, "Partly cloudy"),
    (3.0, "Overcast"),
    (48.0, "Fog"),
    (57.0, "Drizzle"),
    (67.0, "Rain"),
    (77.0, "Snow"),
    (82.0, "Showers"),
    (86.0, "Snow showers"),
    (99.0, "Thunderstorm"),
];

pub fn describe_weather_code(code: f64) -> &'static str {
    CONDITIONS
        .iter()
        .find(|(up_to, _)| code <= *up_to)
        .map_or("Unknown", |(_, label)| label)
}

// The OpenWeatherMap code for each WMO code, since the watch reads skies as Gadgetbridge sends them.
pub fn open_weather_map_code(code: f64) -> Option<u32> {
    if code.fract() != 0.0 {
        return None;
    }
    let mapped = match code as i64 {
        0 => 800,
        1 => 801,
        2 => 802,
        3 => 804,
        45 | 48 => 741,
        51 => 300,
        53 => 301,
        55 => 302,
        56 => 310,
        57 => 311,
        61 => 500,
        63 => 501,
        65 => 502,
        66 | 67 => 511,
        71 | 77 => 600,
        73 => 601,
        75 => 602,
        80 => 520,
        81 => 521,
        82 => 522,
        85 => 620,
        86 => 621,
        95 => 211,
        96 => 201,
        99 => 202,
        _ => return None,
    };
    Some(mapped)
}

// Rounds half up, as JavaScript's Math.round does, so -2.5 becomes -2 rather than -3.
fn round_like_javascript(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

// The watch takes whole kelvin and subtracts 273, so this gives back the rounded Celsius exactly.
pub fn whole_kelvin(celsius: f64) -> i64 {
    round_like_javascript(celsius) + 273
}

// ============================================================================
// Open-Meteo
// ============================================================================

#[derive(Deserialize)]
struct Forecast {
    current: Option<Current>,
    #[serde(default)]
    daily: Option<Daily>,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    relative_humidity_2m: f64,
    weather_code: f64,
    wind_speed_10m: f64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Daily {
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
}

pub fn forecast_url(place: &Location) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m\
         &daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto\
         &latitude={}&longitude={}&wind_speed_unit=kmh",
        place.latitude, place.longitude
    )
}

// The watch's weather message from Open-Meteo's answer for `place`.
pub fn weather_message(place: &Location, body: &[u8]) -> Result<Value, String> {
    let forecast: Forecast = serde_json::from_slice(body)
        .map_err(|error| format!("The weather service sent something unreadable: {error}."))?;
    let Some(now) = forecast.current else {
        return Err("The weather service sent no current conditions.".into());
    };
    let daily = forecast.daily.unwrap_or_default();
    // Today's range, widened to take in the current temperature, or just the current one when the day is missing.
    let temperature = now.temperature_2m;
    let high = daily
        .temperature_2m_max
        .first()
        .copied()
        .unwrap_or(temperature)
        .max(temperature);
    let low = daily
        .temperature_2m_min
        .first()
        .copied()
        .unwrap_or(temperature)
        .min(temperature);
    let mut message = Map::new();
    message.insert("t".into(), json!("weather"));
    message.insert("temp".into(), json!(whole_kelvin(temperature)));
    message.insert("hi".into(), json!(whole_kelvin(high)));
    message.insert("lo".into(), json!(whole_kelvin(low)));
    if let Some(code) = open_weather_map_code(now.weather_code) {
        message.insert("code".into(), json!(code));
    }
    message.insert(
        "hum".into(),
        json!(round_like_javascript(now.relative_humidity_2m)),
    );
    message.insert("txt".into(), json!(describe_weather_code(now.weather_code)));
    message.insert("wind".into(), json!(round_like_javascript(now.wind_speed_10m)));
    message.insert("loc".into(), json!(place.name));
    Ok(Value::Object(message))
}

// ============================================================================
// The weather job
// ============================================================================

pub struct Weather {
    daemon: Arc<Daemon>,
    fetch: WebFetch,
}

impl Weather {
    // Starts sending the weather from Open-Meteo over the real web.
    pub fn start(daemon: &Arc<Daemon>) -> Arc<Weather> {
        Weather::start_with_fetch(daemon, web_fetch())
    }

    pub fn start_with_fetch(daemon: &Arc<Daemon>, fetch: WebFetch) -> Arc<Weather> {
        let weather = Arc::new(Weather {
            daemon: daemon.clone(),
            fetch,
        });
        daemon.runtime().spawn(weather.clone().send_regularly());
        weather
    }

    // Fetches the weather for the chosen place, or where the device is when asked, and sends it now.
    pub async fn send_now(&self) -> Result<(), String> {
        let settings: WeatherSettings = self.daemon.settings().view();
        let place = if settings.weather_from_device {
            self.daemon.device().locate().await?
        } else {
            settings
                .weather_place
                .ok_or("Choose a place for the weather in Settings first.")?
        };
        let reply = (self.fetch)(plain_request("GET", &forecast_url(&place), None)).await?;
        if !(200..300).contains(&reply.status) {
            return Err(format!("The weather service answered {}.", reply.status));
        }
        let message = weather_message(&place, &reply.body)?;
        self.daemon.send(&message).await
    }

    // The link the weather is being sent on, or None while it should not be sent.
    fn sending_for(&self) -> Option<u64> {
        let readiness = self.daemon.readiness().borrow().clone();
        let settings: WeatherSettings = self.daemon.settings().view();
        let has_a_place = settings.weather_place.is_some() || settings.weather_from_device;
        (readiness.uart() && settings.forward_weather && has_a_place).then_some(readiness.epoch)
    }

    // Runs for the daemon's life, sending at once whenever sending turns on and every half hour while it stays on.
    async fn send_regularly(self: Arc<Self>) {
        let mut readiness = self.daemon.readiness();
        let mut settings = self.daemon.settings().subscribe();
        let mut sending_for = None;
        let mut next_send: Option<Instant> = None;
        loop {
            let wanted = self.sending_for();
            if wanted != sending_for {
                sending_for = wanted;
                next_send = wanted.map(|_| Instant::now());
            }
            let due = async {
                match next_send {
                    Some(time) => tokio::time::sleep_until(time).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = due => {
                    if let Err(error) = self.send_now().await {
                        self.daemon.report_error(error);
                    }
                    next_send = next_send.map(|time| time + WEATHER_INTERVAL);
                }
                changed = readiness.changed() => if changed.is_err() { return },
                changed = settings.changed() => if changed.is_err() { return },
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::super::tests::{a_battletime_watch, pass, rig_with};
    use super::*;
    use crate::web_request::{WebReply, WebRequest};

    const FORECAST: &str = r#"{
        "current": { "temperature_2m": 14.6, "relative_humidity_2m": 71.4, "weather_code": 61, "wind_speed_10m": 12.5 },
        "daily": { "temperature_2m_max": [13.0], "temperature_2m_min": [8.2] }
    }"#;

    fn london() -> Location {
        Location {
            latitude: 51.5,
            longitude: -0.12,
            name: "London".into(),
        }
    }

    // A fake Open-Meteo that records each address asked for and always sends `body`.
    fn fake_open_meteo(body: &'static str, asked: Arc<Mutex<Vec<String>>>) -> WebFetch {
        Arc::new(move |outgoing: WebRequest| {
            asked.lock().unwrap().push(outgoing.url);
            Box::pin(async move {
                Ok(WebReply {
                    status: 200,
                    status_text: "OK".into(),
                    body: body.as_bytes().to_vec(),
                })
            })
        })
    }

    #[test]
    fn codes_read_as_words_and_as_openweathermap_skies() {
        assert_eq!(describe_weather_code(0.0), "Clear");
        assert_eq!(describe_weather_code(45.0), "Fog");
        assert_eq!(describe_weather_code(95.0), "Thunderstorm");
        assert_eq!(describe_weather_code(120.0), "Unknown");
        assert_eq!(open_weather_map_code(61.0), Some(500));
        assert_eq!(open_weather_map_code(4.0), None);
    }

    #[test]
    fn temperatures_round_half_up_like_the_window() {
        assert_eq!(whole_kelvin(14.5), 288);
        assert_eq!(whole_kelvin(-2.5), 271);
        assert_eq!(whole_kelvin(-2.6), 270);
    }

    #[test]
    fn the_message_widens_the_days_range_to_take_in_the_current_temperature() {
        let message = weather_message(&london(), FORECAST.as_bytes()).unwrap();
        assert_eq!(
            message,
            json!({
                "t": "weather", "temp": 288, "hi": 288, "lo": 281, "code": 500,
                "hum": 71, "txt": "Rain", "wind": 13, "loc": "London"
            })
        );
    }

    #[test]
    fn an_answer_without_current_conditions_is_an_error() {
        assert_eq!(
            weather_message(&london(), b"{}"),
            Err("The weather service sent no current conditions.".into())
        );
    }

    #[test]
    fn the_forecast_address_names_the_place() {
        assert!(forecast_url(&london()).ends_with("&latitude=51.5&longitude=-0.12&wind_speed_unit=kmh"));
    }

    #[tokio::test(start_paused = true)]
    async fn weather_goes_out_when_the_watch_connects_and_every_half_hour() {
        let rig = rig_with(
            json!({ "forwardWeather": true, "weatherPlace": { "name": "London", "latitude": 51.5, "longitude": -0.12 } }),
            a_battletime_watch,
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        let _weather = Weather::start_with_fetch(&rig.daemon, fake_open_meteo(FORECAST, asked.clone()));
        pass(Duration::from_secs(60)).await;
        assert!(rig.link.sent_of("weather").is_empty());

        rig.daemon.connect("AA", "BattleTime").await.unwrap();
        pass(Duration::from_secs(3)).await;
        assert_eq!(rig.link.sent_of("weather").len(), 1);
        assert_eq!(rig.link.sent_of("weather")[0]["loc"], "London");

        pass(WEATHER_INTERVAL - Duration::from_secs(10)).await;
        assert_eq!(rig.link.sent_of("weather").len(), 1);
        pass(Duration::from_secs(10)).await;
        assert_eq!(rig.link.sent_of("weather").len(), 2);
        assert_eq!(asked.lock().unwrap().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn turning_forwarding_off_stops_it_and_on_again_sends_at_once() {
        let rig = rig_with(
            json!({ "forwardWeather": true, "weatherPlace": { "name": "London", "latitude": 51.5, "longitude": -0.12 } }),
            a_battletime_watch,
        );
        let _weather = Weather::start_with_fetch(&rig.daemon, fake_open_meteo(FORECAST, Default::default()));
        rig.daemon.connect("AA", "BattleTime").await.unwrap();
        pass(Duration::from_secs(3)).await;
        assert_eq!(rig.link.sent_of("weather").len(), 1);

        rig.daemon
            .update_settings(&json!({ "forwardWeather": false }))
            .unwrap();
        pass(WEATHER_INTERVAL * 2).await;
        assert_eq!(rig.link.sent_of("weather").len(), 1);

        rig.daemon
            .update_settings(&json!({ "forwardWeather": true }))
            .unwrap();
        pass(Duration::from_secs(1)).await;
        assert_eq!(rig.link.sent_of("weather").len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn weather_from_the_device_uses_where_it_is_and_a_failure_is_shown() {
        let rig = rig_with(
            json!({ "forwardWeather": true, "weatherFromDevice": true }),
            a_battletime_watch,
        );
        let _weather = Weather::start_with_fetch(&rig.daemon, fake_open_meteo(FORECAST, Default::default()));
        rig.daemon.connect("AA", "BattleTime").await.unwrap();
        pass(Duration::from_secs(3)).await;
        assert!(rig.link.sent_of("weather").is_empty());
        assert_eq!(rig.daemon.state()["lastError"], "nowhere");

        *rig.device.location.lock().unwrap() = Some(Location {
            latitude: 48.85,
            longitude: 2.35,
            name: "Paris".into(),
        });
        let weather = Weather::start_with_fetch(&rig.daemon, fake_open_meteo(FORECAST, Default::default()));
        weather.send_now().await.unwrap();
        assert_eq!(rig.link.sent_of("weather").last().unwrap()["loc"], "Paris");
    }

    #[tokio::test(start_paused = true)]
    async fn sending_now_without_a_place_asks_for_one() {
        let rig = rig_with(Value::Null, a_battletime_watch);
        let weather = Weather::start_with_fetch(&rig.daemon, fake_open_meteo(FORECAST, Default::default()));
        assert_eq!(
            weather.send_now().await,
            Err("Choose a place for the weather in Settings first.".into())
        );
    }
}
