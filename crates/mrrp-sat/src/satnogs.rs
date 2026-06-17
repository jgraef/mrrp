use std::{
    borrow::Cow,
    fs::File,
    io::BufReader,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::DeserializeOwned,
};
use serde_with::{
    NoneAsEmptyString,
    serde_as,
};
use url::Url;

pub const SATNOGS_API_URL: &str = "https://db.satnogs.org/api/";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[derive(Clone, Debug)]
pub struct SatnogsApi {
    client: reqwest::Client,
    base_url: Url,
}

impl SatnogsApi {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("reqwest/mrrp-sat")
            .build()
            .expect("http client");

        Self {
            client,
            base_url: SATNOGS_API_URL.parse().expect("invalid API URL"),
        }
    }

    async fn fetch<T>(&self, endpoint: &str) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        /*Ok(self
        .client
        .get(self.base_url.join(endpoint).expect("invalid endpoint URL"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)*/

        // for testing we'll use a local copy to not spam their API
        let reader = BufReader::new(File::open(&format!("tmp/{}_pretty.json", endpoint)).unwrap());
        Ok(serde_json::from_reader(reader).unwrap())
    }

    pub async fn modes(&self) -> Result<Vec<Mode>, Error> {
        self.fetch("modes").await
    }

    pub async fn satellites(&self) -> Result<Vec<Satellite>, Error> {
        self.fetch("satellites").await
    }

    pub async fn tle(&self) -> Result<Vec<Tle>, Error> {
        self.fetch("tle").await
    }

    pub async fn transmitters(&self) -> Result<Vec<Transmitter>, Error> {
        self.fetch("transmitters").await
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Mode {
    pub id: u64,
    pub name: String,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Satellite {
    pub sat_id: SatelliteId,
    pub norad_cat_id: Option<NoradCatId>,
    pub norad_follow_id: Option<NoradFollowId>,
    pub name: String,
    // todo: the satnogs API returns this a comma-separated list in a string. once we decouple our
    // model from the satnogs model, we can deserialize this into a vec. right now we also
    // serialize this and use this format for our db, which would conflict then.
    #[serde_as(as = "NoneAsEmptyString")]
    pub names: Option<String>,
    #[serde_as(as = "NoneAsEmptyString")]
    pub image: Option<String>,
    pub status: SatelliteStatus,
    pub decayed: Option<DateTime<Utc>>,
    pub launched: Option<DateTime<Utc>>,
    pub deployed: Option<DateTime<Utc>>,
    #[serde_as(as = "NoneAsEmptyString")]
    pub website: Option<Url>,
    #[serde(deserialize_with = "deserialize_operator")]
    pub operator: Option<String>,
    pub countries: String,
    pub telemetries: Vec<Telemetry>,
    pub updated: DateTime<Utc>,
    #[serde(deserialize_with = "deserialize_citation")]
    pub citation: Option<String>,
    pub is_frequency_violator: bool,
    pub associated_satellites: Vec<SatelliteId>,
}

#[derive(
    Clone, Debug, derive_more::Display, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SatelliteId(pub String);

#[derive(
    Clone,
    Copy,
    Debug,
    derive_more::Display,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct NoradCatId(pub u64);

#[derive(
    Clone,
    Copy,
    Debug,
    derive_more::Display,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct NoradFollowId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SatelliteStatus {
    Alive,
    ReEntered,
    Dead,
    Future,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    pub decoder: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tle {
    pub tle0: String,
    pub tle1: String,
    pub tle2: String,
    pub tle_source: String,
    pub sat_id: SatelliteId,
    pub norad_cat_id: Option<NoradCatId>,
    pub updated: DateTime<Utc>,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transmitter {
    pub uuid: String,
    pub description: String,
    pub alive: bool,
    pub r#type: TransmitterType,
    pub uplink_low: Option<u64>,
    pub uplink_high: Option<u64>,
    pub uplink_drift: Option<i64>,
    pub downlink_low: Option<u64>,
    pub downlink_high: Option<u64>,
    pub downlink_drift: Option<i64>,
    pub mode: Option<TransmitterMode>,
    pub mode_id: Option<u64>,
    pub uplink_mode: Option<TransmitterMode>,
    pub invert: bool,
    pub baud: Option<f32>,
    pub sat_id: SatelliteId,
    pub norad_cat_id: NoradCatId,
    pub norad_follow_id: Option<NoradFollowId>,
    pub status: TransmitterStatus,
    pub updated: DateTime<Utc>,
    #[serde(deserialize_with = "deserialize_citation")]
    pub citation: Option<String>,
    pub service: String,
    pub iaru_coordination: String, // todo: Option, None if "N/A"
    #[serde_as(as = "NoneAsEmptyString")]
    pub iaru_coordination_url: Option<Url>,
    pub itu_notification: ItuNotification,
    pub frequency_violation: bool,
    pub unconfirmed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TransmitterType {
    Transceiver,
    Transmitter,
    Transponder,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, derive_more::Display,
)]
#[serde(transparent)]
pub struct TransmitterMode(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransmitterStatus {
    Active,
    Inactive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItuNotification {
    pub urls: Vec<String>,
}

fn deserialize_operator<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<Cow<'de, str>> = Deserialize::deserialize(deserializer)?;

    if let Some(s) = s {
        if s == "None" {
            Ok(None)
        }
        else {
            Ok(Some(s.into_owned()))
        }
    }
    else {
        Ok(None)
    }
}

fn deserialize_citation<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<Cow<'de, str>> = Deserialize::deserialize(deserializer)?;

    if let Some(s) = s {
        if s.contains("CITATION NEEDED") {
            Ok(None)
        }
        else {
            Ok(Some(s.into_owned()))
        }
    }
    else {
        Ok(None)
    }
}
