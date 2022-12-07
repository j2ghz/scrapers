use std::path::PathBuf;

use serde::de::{self};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub scrapers: Vec<ScraperDefinition>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ScraperDefinition {
    pub dest: PathBuf,
    pub urls: Vec<url::Url>,
    pub domain_whitelist: Vec<String>,
    pub steps: Vec<ScraperStep>,
}

#[derive(Deserialize, Debug, Clone)]
pub enum ScraperStep {
    ExtractHrefsFromHTML(HTMLSelector),
    DownloadImage(HTMLSelector),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HTMLSelector(voyager::scraper::Selector);

impl AsRef<voyager::scraper::Selector> for HTMLSelector {
    fn as_ref(&self) -> &voyager::scraper::Selector {
        &self.0
    }
}

impl<'de> Deserialize<'de> for HTMLSelector {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        let selector = voyager::scraper::Selector::parse(&s)
            .map_err(|e| de::Error::custom(format!("Parse error: {e:?}")))?;
        Ok(HTMLSelector(selector))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_example() {
        let config_contents = std::fs::read("test_config.yaml").unwrap();
        let config = &serde_yaml::from_slice::<Config>(&config_contents).unwrap();
        insta::assert_debug_snapshot!(config);
    }
}
