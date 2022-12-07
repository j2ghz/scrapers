use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;
use config::ScraperStep;
use futures::stream::StreamExt;
use futures_timer::Delay;
use sanitize_filename_reader_friendly::sanitize;
use tracing::metadata::LevelFilter;
use tracing::{debug, error, info, warn};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use ureq::Agent;
use urlencoding::decode;
use voyager::scraper::Html;
use voyager::{Collector, Crawler, CrawlerConfig, RequestDelay, Response, Scraper};

use crate::config::ScraperDefinition;

mod config;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Config file
    config: PathBuf,
}

#[tracing::instrument]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    color_eyre::install().unwrap();

    tracing_subscriber::Registry::default()
        .with(tracing_tree::HierarchicalLayer::new(2))
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let cli = Cli::parse();
    debug!(?cli);

    let config_contents = fs::read(cli.config)?;
    let config = &serde_yaml::from_slice::<config::Config>(&config_contents)
        .context("Couldn't parse config")?;
    info!(?config);

    for ScraperDefinition {
        dest,
        urls,
        domain_whitelist,
        steps,
    } in config.scrapers.iter()
    {
        info!("Processing scraper: {}", dest.display());
        let config = CrawlerConfig::default().allow_domains_with_delay(
            domain_whitelist.iter().map(|domain| {
                (
                    domain,
                    RequestDelay::Random {
                        min: std::time::Duration::from_millis(2_000),
                        max: std::time::Duration::from_millis(5_000),
                    },
                )
            }),
        );

        let mut collector = Collector::new(DefaultScraper {}, config);
        debug!(?collector.scraper);

        let crawler = collector.crawler_mut();
        crawler.respects_robots_txt();
        for url in urls {
            crawler.visit_with_state(
                url.clone(),
                State {
                    dest: dest.clone(),
                    steps: steps.clone(),
                },
            );
        }

        let agent: Agent = ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(5))
            .timeout_write(Duration::from_secs(5))
            .build();

        while let Some(output) = collector.next().await {
            let entries = output?;
            info!("Got {} images", entries.len());
            for entry in entries {
                process_image(entry, &agent).await?;
                Delay::new(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn process_image(entry: Download, agent: &Agent) -> anyhow::Result<()> {
    let src = entry.src;
    let dest = &entry.dest;

    if tokio::fs::metadata(dest).await.is_ok() {
        warn!("{} already exists", dest.display());
        return Ok(());
    }

    let parent = dest
        .parent()
        .ok_or_else(|| anyhow!("Can't find root of path: {}", dest.display()))?;
    tokio::fs::create_dir_all(parent).await?;
    let mut file = std::fs::File::create(dest)
        .with_context(|| format!("failed to create file to write to: {}", dest.display()))?;
    match agent.get(src.as_str()).call() {
        Err(e) => error!("Failed to get image: {:?}", e),
        Ok(resp) => {
            let mut reader = resp.into_reader();
            std::io::copy(&mut reader, &mut file).context("failed to write to file")?;
            info!("Downloaded: {}", dest.display());
        }
    }

    Ok(())
}

#[derive(Debug)]
struct DefaultScraper {}

/// The state model
#[derive(Debug)]
struct State {
    dest: PathBuf,
    steps: Vec<ScraperStep>,
}

#[derive(Debug)]
struct Download {
    src: url::Url,
    dest: PathBuf,
}

impl Scraper for DefaultScraper {
    type Output = Vec<Download>;
    type State = State;

    fn scrape(
        &mut self,
        response: Response<Self::State>,
        crawler: &mut Crawler<Self>,
    ) -> anyhow::Result<Option<Self::Output>> {
        if let Some(State { dest, steps }) = response.state {
            if let Some((step, next_steps)) = steps.split_first() {
                match step {
                    ScraperStep::ExtractHrefsFromHTML(selector) => {
                        let html = Html::parse_document(&response.text);
                        for element in html.select(selector.as_ref()) {
                            let href = element
                                .value()
                                .attr("href")
                                .ok_or_else(|| anyhow!("Failed to find href on: {:?}", element))?;

                            let next_name = element.text().fold(String::new(), |mut state, str| {
                                state.push_str(str);
                                state
                            });

                            let path = dest.join(sanitize(&next_name));

                            if path.exists() {
                                info!("Skipping: {} (Already exists)", next_name);
                                continue;
                            }

                            info!("Found: {}", next_name);

                            crawler.visit_with_state(
                                response.request_url.join(href).with_context(|| {
                                    format!(
                                        "Failed to make url form base: '{}' and href: '{}'",
                                        response.request_url, href
                                    )
                                })?,
                                State {
                                    dest: path,
                                    steps: next_steps.to_vec(),
                                },
                            );
                        }
                        return Ok(None);
                    }
                    ScraperStep::DownloadImage(selector) => {
                        let html = Html::parse_document(&response.text);
                        let mut results = Vec::new();
                        for (i, element) in html.select(selector.as_ref()).enumerate() {
                            let src = element
                                .value()
                                .attr("src")
                                .ok_or_else(|| anyhow!("Failed to find src on: {:?}", element))?;
                            let src = response.request_url.join(src)?;

                            let img_name = decode(src.path_segments().unwrap().last().unwrap())?;

                            let path = dest.join(format!("{:03}-{}", i, sanitize(&img_name)));

                            if path.exists() {
                                info!("Skipping: {} (Already exists)", img_name);
                                continue;
                            }

                            info!("To download: {}", src);

                            results.push(Download { src, dest: path });
                        }
                        return Ok(Some(results));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {}
