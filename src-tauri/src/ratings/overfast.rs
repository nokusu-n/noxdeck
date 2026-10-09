//! Everything known about the rating service is confined to this module.
use super::RatingValue;
use std::time::Duration;
use ureq::{Agent, tls::{RootCerts, TlsConfig, TlsProvider}};

pub const TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Debug, PartialEq, Eq)]
pub enum Report { Ratings([Option<RatingValue>; 3]), Stop }

pub struct Client { base: String, agent: Agent }
impl Client {
    pub fn from_env() -> Self {
        Self::new(&std::env::var("NOXDECK_OVERFAST_URL").unwrap_or_else(|_| "https://overfast-api.tekrop.fr".into()))
    }
    pub fn new(base: &str) -> Self { Self::with_timeout(base, TIMEOUT) }
    pub fn with_timeout(base: &str, timeout: Duration) -> Self {
        let agent = Agent::config_builder()
            .tls_config(TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build())
            .timeout_global(Some(timeout)).max_redirects(0)
            .http_status_as_error(false).proxy(None).user_agent("")
            .max_idle_connections(0)
            .build().new_agent();
        Self { base: base.to_owned(), agent }
    }
    pub fn fetch(&self, battletag: &str) -> Report {
        self.request(battletag).unwrap_or(Report::Ratings([None, None, None]))
    }
    fn request(&self, battletag: &str) -> Option<Report> {
        let mut url = tauri::Url::parse(&self.base).ok()?;
        url.path_segments_mut().ok()?.pop_if_empty().push("players").push(&battletag.replace('#', "-")).push("summary");
        let mut response = self.agent.get(url.as_str()).call().ok()?;
        match response.status().as_u16() {
            429 | 503 => return Some(Report::Stop),
            200 => (),
            _ => return None,
        }
        let body = response.body_mut().read_to_string().ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        let pc = &json["competitive"]["pc"];
        Some(Report::Ratings(["tank", "damage", "support"].map(|role| {
            let value = RatingValue {
                rank: pc[role]["division"].as_str()?.to_owned(),
                division: Some(u8::try_from(pc[role]["tier"].as_u64()?).ok()?),
            };
            (value.rank != "placement" && value.valid()).then_some(value)
        })))
    }
}
