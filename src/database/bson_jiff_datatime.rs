pub mod bson_jiff_timestamp {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Timestamp, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bson::DateTime::from_millis(value.as_millisecond()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Timestamp, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = bson::DateTime::deserialize(deserializer)?;
        Timestamp::from_millisecond(value.timestamp_millis()).map_err(serde::de::Error::custom)
    }
}

pub mod bson_jiff_timestamp_option {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<Timestamp>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value
            .map(|v| bson::DateTime::from_millis(v.as_millisecond()))
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Timestamp>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<bson::DateTime>::deserialize(deserializer)?
            .map(|v| Timestamp::from_millisecond(v.timestamp_millis()))
            .transpose()
            .map_err(serde::de::Error::custom)
    }
}
