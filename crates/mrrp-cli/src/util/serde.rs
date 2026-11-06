pub mod human_time_delta {
    use chrono::TimeDelta;
    use serde::{
        Deserializer,
        Serializer,
        ser::Error as _,
    };

    pub fn serialize<S>(value: &TimeDelta, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = value.to_std().map_err(S::Error::custom)?;
        humantime_serde::serialize(&duration, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TimeDelta, D::Error>
    where
        D: Deserializer<'de>,
    {
        let duration = humantime_serde::deserialize(deserializer)?;
        TimeDelta::from_std(duration).map_err(serde::de::Error::custom)
    }
}

pub mod regex {
    use std::borrow::Cow;

    use regex::Regex;
    use serde::{
        Deserialize,
        Deserializer,
        Serialize,
        Serializer,
    };

    pub fn serialize<S>(value: &Regex, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_str().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Regex, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
