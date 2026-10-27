//! SatNOGS API client
//!
//! [API schema][1]
//!
//! [1]: https://db.satnogs.org/api/schema/docs/#/

use std::borrow::Cow;

use chrono::{
    DateTime,
    Utc,
};
use reqwest::{
    StatusCode,
    header,
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

/// create endpoint URL
fn endpoint_url(base_url: &Url, endpoint: &str) -> Url {
    let mut url = base_url.clone();
    let mut path_segments = url.path_segments_mut().unwrap();
    path_segments.pop_if_empty();
    for segment in endpoint.split('/') {
        path_segments.push(segment);
    }
    // the API expects a trailing /
    path_segments.push("");
    drop(path_segments);
    url
}

/// normalize base URL
fn normalize_base_url(mut base_url: Url) -> Result<Url, Error> {
    //
    // note: unfortunately the borrow checker is a bit annoying here. we can
    // only return the error outside of the match.
    let mut ok = true;
    match base_url.path_segments_mut() {
        Ok(mut path_segments) => {
            path_segments.pop_if_empty();
        }
        Err(()) => {
            ok = false;
        }
    }

    if ok {
        Ok(base_url)
    }
    else {
        Err(Error::InvalidUrl { url: base_url })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("Invalid URL: {url}")]
    InvalidUrl { url: Url },
}

#[derive(Clone, Debug)]
pub struct SatnogsApi {
    client: reqwest::Client,
    base_url: Url,
}

impl SatnogsApi {
    pub fn new() -> Self {
        let base_url = SATNOGS_API_URL
            .parse()
            .expect("`SATNOGS_API_URL` is invalid");
        Self::from_base_url(base_url).expect("Failed to create API client from `SATNOGS_API_URL`")
    }

    pub fn from_base_url(base_url: Url) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .user_agent("reqwest/mrrp-sat")
            .build()
            .expect("http client");

        let base_url = normalize_base_url(base_url)?;

        Ok(Self {
            client,
            base_url,
        })
    }

    async fn fetch<T>(
        &self,
        endpoint: &str,
        if_modified_since: Option<DateTime<Utc>>,
    ) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        let request = self.client.get(endpoint_url(&self.base_url, endpoint));

        let request = if let Some(if_modified_since) = if_modified_since {
            request.header(
                header::IF_MODIFIED_SINCE,
                if_modified_since
                    .format("%a, %d %b %Y %H:%M:%S GMT")
                    .to_string(),
            )
        }
        else {
            request
        };

        let response = request.send().await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            tracing::debug!(%endpoint, "not modified");
            Ok(None)
        }
        else {
            Ok(response.error_for_status()?.json().await?)
        }
    }

    #[inline]
    pub async fn modes(
        &self,
        if_modified_since: Option<DateTime<Utc>>,
    ) -> Result<Option<Vec<Mode>>, Error> {
        self.fetch("modes", if_modified_since).await
    }

    #[inline]
    pub async fn satellites(
        &self,
        if_modified_since: Option<DateTime<Utc>>,
    ) -> Result<Option<Vec<Satellite>>, Error> {
        self.fetch("satellites", if_modified_since).await
    }

    #[inline]
    pub async fn tle(
        &self,
        if_modified_since: Option<DateTime<Utc>>,
    ) -> Result<Option<Vec<Tle>>, Error> {
        self.fetch("tle", if_modified_since).await
    }

    #[inline]
    pub async fn transmitters(
        &self,
        if_modified_since: Option<DateTime<Utc>>,
    ) -> Result<Option<Vec<Transmitter>>, Error> {
        self.fetch("transmitters", if_modified_since).await
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
    // todo: the satnogs API returns this a comma-separated list in a string.
    // once we decouple our model from the satnogs model, we can deserialize
    // this into a vec. right now we also serialize this and use this format for
    // our db, which would conflict then.
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
    Clone,
    Debug,
    derive_more::Display,
    derive_more::FromStr,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
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
    #[serde(rename = "in orbit")]
    InOrbit,
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

#[cfg(test)]
mod tests {
    use crate::satnogs::{
        SATNOGS_API_URL,
        endpoint_url,
        normalize_base_url,
    };

    #[test]
    fn it_creates_well_formed_endpoint_urls() {
        let url = endpoint_url(&SATNOGS_API_URL.parse().unwrap(), "tle");
        assert_eq!(url.to_string(), "https://db.satnogs.org/api/tle/");

        let url = endpoint_url(&SATNOGS_API_URL.parse().unwrap(), "tle/historical");
        assert_eq!(
            url.to_string(),
            "https://db.satnogs.org/api/tle/historical/"
        );
    }

    #[test]
    fn it_normalizes_base_urls() {
        let url = normalize_base_url("https://alt.satnogs.db/a/b".parse().unwrap()).unwrap();
        assert_eq!(url.to_string(), "https://alt.satnogs.db/a/b");

        let url = normalize_base_url("https://alt.satnogs.db/a/b/".parse().unwrap()).unwrap();
        assert_eq!(url.to_string(), "https://alt.satnogs.db/a/b");
    }
}
