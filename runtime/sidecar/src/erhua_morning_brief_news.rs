//! Erhua morning-brief news fetch (Rust migration of the previously-Python logic).
//!
//! This module owns the news domain end to end: it fetches the configured RSS
//! feeds over the shared bounded HTTP client, parses them, applies a publish-date
//! recency window, suppresses titles sent in the recent past via a persistent
//! history file, and emits the surviving items as JSON. Python stays a thin
//! orchestrator that shells out here and then translates/renders the brief.
//!
//! The history file is written by a *separate* command
//! (`OperationsErhuaMorningBriefNewsRecord`) so that the dedup record is only
//! persisted once the morning brief has actually been published. A failed
//! render/artifact/send must never mark titles as sent.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, Days, Duration, NaiveDate, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration as StdDuration;
use url::Url;

use crate::bounded_http::HttpClient;
use crate::config::Cli;

pub const DEFAULT_NEWS_LIMIT: usize = 5;
pub const DEFAULT_NEWS_FEED_TIMEOUT_SECONDS: u64 = 12;
pub const DEFAULT_NEWS_DEDUP_DAYS: i64 = 7;

const NEWS_FEED_MAX_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_NEWS_FEED_URLS: &[&str] = &[
    "https://openai.com/news/rss.xml",
    "https://blog.google/technology/ai/rss/",
    "https://deepmind.google/blog/rss.xml",
    "https://huggingface.co/blog/feed.xml",
    "https://arxiv.org/rss/cs.AI",
];
const NEWS_FEED_ALLOWED_HOSTS: &[&str] = &[
    "openai.com",
    "blog.google",
    "deepmind.google",
    "huggingface.co",
    "arxiv.org",
];
const NEWS_USER_AGENT: &str = "qintopia-erhua-morning-brief/1.0";

#[derive(Debug, Serialize, Clone)]
pub struct NewsItem {
    pub title: String,
    pub summary: String,
    /// RFC 3339 publish time when the feed provided one, else null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

struct RawItem {
    title: String,
    summary: String,
    published: Option<String>,
}

pub async fn run_news_fetch(
    _cli: &Cli,
    feed_urls: Vec<String>,
    feed_timeout_seconds: u64,
    limit: usize,
    recency_days: i64,
    dedup_days: i64,
    history_path: String,
) -> Result<()> {
    let json = news_fetch_core(
        feed_urls,
        feed_timeout_seconds,
        limit,
        recency_days,
        dedup_days,
        history_path,
    )?;
    println!("{json}");
    Ok(())
}

/// Pure fetch+filter logic. Returns the JSON document the CLI would print.
pub fn news_fetch_core(
    feed_urls: Vec<String>,
    feed_timeout_seconds: u64,
    limit: usize,
    recency_days: i64,
    dedup_days: i64,
    history_path: String,
) -> Result<String> {
    if feed_timeout_seconds == 0 {
        bail!("news feed timeout must be positive");
    }
    if limit == 0 {
        bail!("news limit must be positive");
    }

    let feeds: Vec<Url> = if feed_urls.is_empty() {
        DEFAULT_NEWS_FEED_URLS
            .iter()
            .map(|u| Url::parse(u).expect("default news feed url must parse"))
            .collect()
    } else {
        feed_urls
            .iter()
            .map(|u| Url::parse(u).with_context(|| "parse news feed url"))
            .collect::<Result<Vec<Url>>>()?
    };
    for url in &feeds {
        if !is_allowed_news_host(url) {
            bail!(
                "news feed host is not allowlisted: {}",
                url.host_str().unwrap_or("")
            );
        }
    }

    let client = HttpClient::production_with_timeout(StdDuration::from_secs(feed_timeout_seconds));
    let recency_cutoff: Option<DateTime<Utc>> = if recency_days > 0 {
        Some(Utc::now() - Duration::days(recency_days))
    } else {
        None
    };
    // When recency or dedup is on, fetch more than the brief limit so old pinned
    // or already-sent entries can be filtered out without starving the brief.
    let fetch_cap = if recency_days > 0 || (dedup_days > 0 && !history_path.is_empty()) {
        limit.saturating_mul(4).max(20)
    } else {
        limit
    };

    // Dedup set, loaded once up front so we can count fresh candidates inline
    // and decide whether to keep requesting further feeds.
    let recent_lower: Option<HashSet<String>> = if dedup_days > 0 && !history_path.is_empty() {
        let recent = load_recent_titles(Path::new(&history_path), dedup_days)?;
        Some(recent.iter().map(|t| t.to_lowercase()).collect())
    } else {
        None
    };

    let mut feeds_xml: Vec<(Url, String)> = Vec::with_capacity(feeds.len());
    for url in &feeds {
        let response = client
            .request(
                "GET",
                url,
                &[("User-Agent", NEWS_USER_AGENT.to_string())],
                &[],
                NEWS_FEED_MAX_BYTES,
            )
            .map_err(|e| e.into_source())
            .with_context(|| format!("fetch news feed {url}"))?;
        // The bounded client never follows redirects, so a 3xx is treated as a
        // dead feed (mirrors the Python NoNewsFeedRedirect behavior).
        if response.status != 200 {
            continue;
        }
        feeds_xml.push((
            url.clone(),
            String::from_utf8_lossy(&response.body).into_owned(),
        ));
    }

    let items = select_news_items(
        &feeds_xml,
        recency_cutoff,
        recent_lower.as_ref(),
        limit,
        fetch_cap,
    );
    serde_json::to_string(&items).context("serialize news items")
}

/// Pure feed selection: apply recency + cross-day dedup and keep at most
/// ``limit`` items. No IO, so it is unit-testable and shared with the CLI path.
///
/// Stopping rule: when dedup is enabled we only stop requesting further feeds
/// once we have enough *fresh* (not recently sent) candidates. A feed full of
/// already-sent items contributes zero fresh candidates, so we must keep reading
/// subsequent feeds to refill the brief instead of bailing out early on the raw
/// candidate count. When dedup is off, the raw candidate count is the bound.
pub fn select_news_items(
    feeds_xml: &[(Url, String)],
    recency_cutoff: Option<DateTime<Utc>>,
    recent_lower: Option<&HashSet<String>>,
    limit: usize,
    fetch_cap: usize,
) -> Vec<NewsItem> {
    // Memory bound for a single feed's raw items. With dedup on we allow scanning
    // well past fetch_cap per feed, because a feed full of already-sent items
    // yields zero fresh candidates and we must keep reading to find new ones.
    let raw_cap = if recent_lower.is_some() {
        fetch_cap.saturating_mul(8).max(60)
    } else {
        fetch_cap
    };

    let mut collected: Vec<NewsItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut fresh_count: usize = 0;
    'feeds: for (_, xml) in feeds_xml {
        for raw in parse_feed_items(xml, recency_cutoff) {
            let key = raw.title.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key.clone());
            let is_fresh = match recent_lower {
                Some(recent) => !recent.contains(&key),
                None => true,
            };
            if is_fresh {
                fresh_count += 1;
            }
            collected.push(NewsItem {
                title: raw.title,
                summary: raw.summary,
                published: raw.published,
            });
            // Bound raw memory per feed.
            if collected.len() >= raw_cap {
                break;
            }
            // Stop requesting further feeds only once we actually have enough
            // *fresh* candidates. With dedup on, the raw count alone must not stop
            // us: an all-sent feed yields zero fresh and we must move on to refill.
            if recent_lower.is_some() {
                if fresh_count >= fetch_cap {
                    break 'feeds;
                }
            } else if collected.len() >= fetch_cap {
                break 'feeds;
            }
        }
    }

    let mut items = collected;
    if let Some(recent) = recent_lower {
        items.retain(|it| !recent.contains(&it.title.to_lowercase()));
    }
    items.truncate(limit);
    items
}

pub async fn run_news_record(
    _cli: &Cli,
    date: String,
    titles_json: String,
    history_path: String,
    dedup_days: i64,
) -> Result<()> {
    news_record_core(date, titles_json, history_path, dedup_days)
}

/// Pure history-record logic (the success-boundary write).
pub fn news_record_core(
    date: String,
    titles_json: String,
    history_path: String,
    dedup_days: i64,
) -> Result<()> {
    if history_path.is_empty() {
        bail!("news history path is required to record sent titles");
    }
    let titles: Vec<String> = serde_json::from_str(&titles_json)
        .with_context(|| "news titles must be a JSON array of strings")?;
    let path = Path::new(&history_path);
    let mut data = load_history(path)?;
    let entry = data.entry(date.clone()).or_default();
    for title in titles {
        let t = title.trim().to_string();
        if t.is_empty() || entry.iter().any(|e| e.eq_ignore_ascii_case(&t)) {
            continue;
        }
        entry.push(t);
    }
    if dedup_days > 0 {
        prune_history(&mut data, dedup_days);
    }
    write_history(path, &data)
}

fn is_allowed_news_host(url: &Url) -> bool {
    match url.host_str() {
        Some(host) => NEWS_FEED_ALLOWED_HOSTS.contains(&host),
        None => false,
    }
}

fn parse_feed_items(xml: &str, cutoff: Option<DateTime<Utc>>) -> Vec<RawItem> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut items: Vec<RawItem> = Vec::new();

    let mut in_item = false;
    let mut title = String::new();
    let mut summary = String::new();
    let mut pub_date = String::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if tag == "item" || tag == "entry" {
                    in_item = true;
                    title.clear();
                    summary.clear();
                    pub_date.clear();
                }
                text.clear();
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if tag == "item" || tag == "entry" {
                    if in_item {
                        let parsed = parse_pub_date(&pub_date);
                        let keep = match (parsed, cutoff) {
                            (_, None) => true,
                            (None, Some(_)) => true,
                            (Some(dt), Some(c)) => dt >= c,
                        };
                        if keep {
                            items.push(RawItem {
                                title: title.trim().to_string(),
                                summary: summary.trim().to_string(),
                                published: parsed.map(|d| d.to_rfc3339()),
                            });
                        }
                    }
                    in_item = false;
                } else if in_item {
                    match tag.as_str() {
                        "title" => title = text.trim().to_string(),
                        "description" | "summary" => summary = text.trim().to_string(),
                        "pubdate" | "updated" | "published" => pub_date = text.trim().to_string(),
                        _ => {}
                    }
                }
                text.clear();
            }
            Ok(Event::Text(e)) => {
                if in_item {
                    if let Ok(decoded) = e.unescape() {
                        text.push_str(&decoded);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    items
}

fn parse_pub_date(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        })
}

fn load_history(path: &Path) -> Result<HashMap<String, Vec<String>>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read news history {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str(&text).with_context(|| "parse news history json")
}

fn write_history(path: &Path, data: &HashMap<String, Vec<String>>) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create news history dir {}", parent.display()))?;
        }
    }
    let bytes = serde_json::to_vec_pretty(data)?;
    std::fs::write(path, bytes)
        .with_context(|| format!("write news history {}", path.display()))?;
    Ok(())
}

fn load_recent_titles(path: &Path, dedup_days: i64) -> Result<HashSet<String>> {
    let data = load_history(path)?;
    let cutoff = date_key_offset(dedup_days);
    let mut recent: HashSet<String> = HashSet::new();
    for (key, titles) in data.iter() {
        if let Ok(date) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
            if date < cutoff {
                continue;
            }
        }
        for title in titles {
            recent.insert(title.clone());
        }
    }
    Ok(recent)
}

fn date_key_offset(days: i64) -> NaiveDate {
    let today = NaiveDate::from_ymd_opt(Utc::now().year(), Utc::now().month(), Utc::now().day())
        .expect("current date is valid");
    today - Days::new(days.unsigned_abs())
}

fn prune_history(data: &mut HashMap<String, Vec<String>>, dedup_days: i64) {
    let cutoff = date_key_offset(dedup_days);
    data.retain(|key, _| {
        NaiveDate::parse_from_str(key, "%Y-%m-%d")
            .map(|d| d >= cutoff)
            .unwrap_or(false)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rss_with(pub_dates: &[Option<&str>]) -> String {
        let mut items = String::new();
        for (i, pd) in pub_dates.iter().enumerate() {
            let title = format!("条目{i}");
            match pd {
                Some(d) => items.push_str(&format!(
                    "<item><title>{title}</title><description>d{i}</description><pubDate>{d}</pubDate></item>"
                )),
                None => items.push_str(&format!(
                    "<item><title>{title}</title><description>d{i}</description></item>"
                )),
            }
        }
        format!("<rss><channel>{items}</channel></rss>")
    }

    #[test]
    fn recency_drops_items_older_than_window() {
        let now = Utc::now();
        let recent = (now - Duration::days(1))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        let old = (now - Duration::days(2000))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        let xml = rss_with(&[Some(&old), Some(&recent)]);
        let cutoff = Some(now - Duration::days(14));
        let items = parse_feed_items(&xml, cutoff);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "条目1");
    }

    #[test]
    fn undated_items_kept_when_recency_on() {
        let xml = rss_with(&[None]);
        let cutoff = Some(Utc::now() - Duration::days(14));
        let items = parse_feed_items(&xml, cutoff);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn recency_prefilter_keeps_newer_items_when_old_come_first() {
        let now = Utc::now();
        let old = (now - Duration::days(2000))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        let new_a = (now - Duration::days(1))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        let new_b = (now - Duration::days(2))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        let xml = format!(
            "<rss><channel>\
<item><title>旧一</title><description>x</description><pubDate>{old}</pubDate></item>\
<item><title>旧二</title><description>x</description><pubDate>{old}</pubDate></item>\
<item><title>新A</title><description>x</description><pubDate>{new_a}</pubDate></item>\
<item><title>新B</title><description>x</description><pubDate>{new_b}</pubDate></item>\
</channel></rss>"
        );
        let cutoff = Some(now - Duration::days(14));
        let items = parse_feed_items(&xml, cutoff);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, vec!["新A", "新B"]);
    }

    #[test]
    fn merge_today_preserves_order_and_dedupes_on_record() {
        let dir = std::env::temp_dir().join(format!("erhua-news-test-{}", uuid_now()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hist.json");
        let _ = std::fs::remove_file(&path);
        let history_path = path.to_string_lossy().to_string();

        news_record_core(
            "2026-08-20".to_string(),
            serde_json::to_string(&vec!["甲", "乙"]).unwrap(),
            history_path.clone(),
            7,
        )
        .unwrap();
        // A same-day re-run with no new titles must merge, not overwrite.
        news_record_core(
            "2026-08-20".to_string(),
            serde_json::to_string(&vec!["甲", "乙"]).unwrap(),
            history_path.clone(),
            7,
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let map: HashMap<String, Vec<String>> = serde_json::from_str(&text).unwrap();
        assert_eq!(
            map.get("2026-08-20").unwrap(),
            &vec!["甲".to_string(), "乙".to_string()]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_does_not_wipe_history_on_empty_same_day_rerun() {
        let dir = std::env::temp_dir().join(format!("erhua-news-test-{}", uuid_now()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hist.json");
        let _ = std::fs::remove_file(&path);
        let history_path = path.to_string_lossy().to_string();

        news_record_core(
            "2026-08-20".to_string(),
            serde_json::to_string(&vec!["甲", "乙"]).unwrap(),
            history_path.clone(),
            7,
        )
        .unwrap();
        // A same-day re-run that would record nothing must leave the day intact.
        news_record_core(
            "2026-08-20".to_string(),
            serde_json::to_string(&Vec::<String>::new()).unwrap(),
            history_path.clone(),
            7,
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let map: HashMap<String, Vec<String>> = serde_json::from_str(&text).unwrap();
        assert_eq!(
            map.get("2026-08-20").unwrap(),
            &vec!["甲".to_string(), "乙".to_string()]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dedup_refill_pulls_from_later_feeds_when_first_is_all_sent() {
        let now = Utc::now();
        let pd = (now - Duration::days(1))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        // First feed: 20 items, all already sent (live in the dedup set).
        let mut first = String::new();
        let mut recent: HashSet<String> = HashSet::new();
        for i in 0..20 {
            let t = format!("已发{i}");
            recent.insert(t.to_lowercase());
            first.push_str(&format!(
                "<item><title>{t}</title><description>d</description><pubDate>{pd}</pubDate></item>"
            ));
        }
        // Second feed: fresh items that must refill the brief.
        let mut second = String::new();
        for i in 0..3 {
            let t = format!("新条目{i}");
            second.push_str(&format!(
                "<item><title>{t}</title><description>d</description><pubDate>{pd}</pubDate></item>"
            ));
        }
        let feeds = vec![
            (
                Url::parse("https://openai.com/news/rss.xml").unwrap(),
                format!("<rss><channel>{first}</channel></rss>"),
            ),
            (
                Url::parse("https://huggingface.co/blog/feed.xml").unwrap(),
                format!("<rss><channel>{second}</channel></rss>"),
            ),
        ];
        // fetch_cap=20 but only 3 fresh arrive; we must still scan the 2nd feed.
        let items = select_news_items(&feeds, Some(now - Duration::days(14)), Some(&recent), 5, 20);
        let titles: Vec<String> = items.iter().map(|i| i.title.clone()).collect();
        assert_eq!(
            titles,
            vec![
                "新条目0".to_string(),
                "新条目1".to_string(),
                "新条目2".to_string()
            ]
        );
    }

    #[test]
    fn dedup_stops_at_fresh_cap_not_raw_cap() {
        let now = Utc::now();
        let pd = (now - Duration::days(1))
            .format("%a, %d %b %Y %H:%M:%S +0000")
            .to_string();
        // First feed: 20 fresh items already satisfy fetch_cap.
        let mut first = String::new();
        for i in 0..20 {
            first.push_str(&format!(
                "<item><title>鲜{i}</title><description>d</description><pubDate>{pd}</pubDate></item>"
            ));
        }
        // Second feed: must NOT be read because the first already gave enough fresh.
        let mut second = String::new();
        for i in 0..5 {
            second.push_str(&format!(
                "<item><title>不应出现{i}</title><description>d</description><pubDate>{pd}</pubDate></item>"
            ));
        }
        let feeds = vec![
            (
                Url::parse("https://openai.com/news/rss.xml").unwrap(),
                format!("<rss><channel>{first}</channel></rss>"),
            ),
            (
                Url::parse("https://arxiv.org/rss/cs.AI").unwrap(),
                format!("<rss><channel>{second}</channel></rss>"),
            ),
        ];
        let items = select_news_items(&feeds, Some(now - Duration::days(14)), None, 5, 20);
        assert!(items.len() <= 5);
        assert!(items.iter().all(|i| i.title.starts_with("鲜")));
    }

    fn uuid_now() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
