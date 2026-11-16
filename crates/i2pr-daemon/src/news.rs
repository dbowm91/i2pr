//! Bounded parser for authenticated router NEWS Atom feeds.

use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::addressbook_fetch::{BoundedContentFetcher, FetchResponse};
use crate::config::NewsConfig;
use i2pr_storage::VerifiedContentCacheStore;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

pub(crate) const NEWS_CONTENT_TYPE: u8 = 4;
pub(crate) const NEWS_FILE_TYPE_XML: u8 = 1;
pub(crate) const NEWS_FILE_TYPE_GZIP_XML: u8 = 3;
pub(crate) const MAX_NEWS_SU3_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_NEWS_XML_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_NEWS_ENTRIES: usize = 256;
pub(crate) const MAX_NEWS_DEPTH: usize = 24;
pub(crate) const MAX_NEWS_TEXT_BYTES: usize = 512 * 1024;
pub(crate) const MAX_NEWS_ENTRY_TEXT_BYTES: usize = 8 * 1024;
const ATOM_NAMESPACE: &str = "http://www.w3.org/2005/Atom";
const CACHE_MAGIC: &[u8; 8] = b"I2PNEWS\0";
const CACHE_VERSION: u16 = 1;
const MAX_NEWS_CERTIFICATE_BYTES: usize = 64 * 1024;
const NEWS_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewsFeed {
    pub(crate) updated: String,
    pub(crate) entries: Vec<NewsEntry>,
    pub(crate) rendered: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewsEntry {
    pub(crate) updated: String,
    pub(crate) title: String,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NewsError {
    WrongContainerType,
    CompressedTooLarge,
    Decompression,
    XmlTooLarge,
    MalformedXml,
    InvalidAtom,
    InvalidTimestamp,
    TooManyEntries,
    TooDeep,
    TextTooLarge,
    MissingField,
    InvalidCache,
    CacheUnavailable,
    CertificateUnavailable,
    UntrustedSignature,
    FetchUnavailable,
    RefreshDisabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NewsRefreshResult {
    Updated,
    NotModified,
    Failed(NewsError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewsSnapshot {
    pub(crate) rendered: String,
    pub(crate) updated: String,
    pub(crate) verified_at_unix: u64,
    pub(crate) last_attempt_unix: u64,
    pub(crate) stale: bool,
    pub(crate) last_error: Option<NewsError>,
}

#[derive(Clone, Debug)]
struct CachedNews {
    su3: Vec<u8>,
    etag: Option<String>,
    last_modified: Option<String>,
    feed: NewsFeed,
    verified_at_unix: u64,
}

pub(crate) struct NewsManager {
    config: NewsConfig,
    data_dir: PathBuf,
    fetcher: Arc<dyn BoundedContentFetcher>,
    state: Mutex<NewsState>,
    signer: Mutex<Option<i2pr_su3::RsaSha512Signer>>,
    refresh_lock: tokio::sync::Mutex<()>,
    initialized: Mutex<bool>,
}

#[derive(Default)]
struct NewsState {
    current: Option<CachedNews>,
    last_attempt_unix: u64,
    last_error: Option<NewsError>,
}

impl NewsManager {
    pub(crate) fn new(
        config: NewsConfig,
        data_dir: PathBuf,
        fetcher: Arc<dyn BoundedContentFetcher>,
    ) -> Self {
        Self {
            config,
            data_dir,
            fetcher,
            state: Mutex::new(NewsState::default()),
            signer: Mutex::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
            initialized: Mutex::new(false),
        }
    }

    pub(crate) fn snapshot(&self, now: u64) -> Option<NewsSnapshot> {
        let state = self.state.lock().ok()?;
        let cached = state.current.as_ref()?;
        Some(NewsSnapshot {
            rendered: cached.feed.rendered.clone(),
            updated: cached.feed.updated.clone(),
            verified_at_unix: cached.verified_at_unix,
            last_attempt_unix: state.last_attempt_unix,
            stale: now.saturating_sub(cached.verified_at_unix)
                > self.config.refresh_interval.as_secs().saturating_mul(2),
            last_error: state.last_error,
        })
    }

    pub(crate) async fn refresh_once(&self) -> NewsRefreshResult {
        if !self.config.enabled {
            return self.record_failure(NewsError::RefreshDisabled);
        }
        let _serial = self.refresh_lock.lock().await;
        if self.initialize().is_err() {
            return self.record_failure(NewsError::CertificateUnavailable);
        }
        let previous = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.current.clone());
        let url = match self.config.source_url.as_deref() {
            Some(url) => url,
            None => return self.record_failure(NewsError::RefreshDisabled),
        };
        let response = self
            .fetcher
            .fetch_bounded(
                url,
                previous.as_ref().and_then(|cache| cache.etag.as_deref()),
                previous
                    .as_ref()
                    .and_then(|cache| cache.last_modified.as_deref()),
                self.config.max_su3_bytes,
                NEWS_FETCH_TIMEOUT,
            )
            .await;
        let response = match response {
            Ok(response) => response,
            Err(_) => return self.record_failure(NewsError::FetchUnavailable),
        };
        match response.status {
            304 => self.commit_not_modified(previous, response),
            200 => self.commit_new_content(response),
            _ => NewsRefreshResult::Failed(NewsError::FetchUnavailable),
        }
    }

    fn initialize(&self) -> Result<(), NewsError> {
        if self.initialized.lock().map(|state| *state).unwrap_or(false) {
            return Ok(());
        }
        let signer_id = self
            .config
            .signer_id
            .as_deref()
            .ok_or(NewsError::CertificateUnavailable)?;
        let certificate_path = self
            .config
            .certificate_path
            .as_ref()
            .ok_or(NewsError::CertificateUnavailable)?;
        let metadata = fs::symlink_metadata(certificate_path)
            .map_err(|_| NewsError::CertificateUnavailable)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_NEWS_CERTIFICATE_BYTES as u64
        {
            return Err(NewsError::CertificateUnavailable);
        }
        let file = File::open(certificate_path).map_err(|_| NewsError::CertificateUnavailable)?;
        let mut certificate = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_NEWS_CERTIFICATE_BYTES + 1) as u64)
            .read_to_end(&mut certificate)
            .map_err(|_| NewsError::CertificateUnavailable)?;
        if certificate.is_empty() || certificate.len() > MAX_NEWS_CERTIFICATE_BYTES {
            return Err(NewsError::CertificateUnavailable);
        }
        let signer = i2pr_su3::rsa_signer_from_certificate(signer_id, &certificate)
            .map_err(|_| NewsError::CertificateUnavailable)?;
        if let Ok(mut slot) = self.signer.lock() {
            *slot = Some(signer);
        } else {
            return Err(NewsError::CertificateUnavailable);
        }
        let store = VerifiedContentCacheStore::in_data_dir(&self.data_dir);
        let mut loaded = None;
        for record in [store.load_current(), store.load_backup()] {
            if let Ok(Some(record)) = record
                && let Ok(record) = decode_cache_record(&record, self.config.max_su3_bytes)
                && let Ok((feed, verified_at_unix)) = self.verify_record(&record)
            {
                loaded = Some(CachedNews {
                    su3: record.su3,
                    etag: record.etag,
                    last_modified: record.last_modified,
                    feed,
                    verified_at_unix,
                });
                break;
            }
        }
        if let Ok(mut state) = self.state.lock() {
            state.current = loaded;
        }
        if let Ok(mut initialized) = self.initialized.lock() {
            *initialized = true;
        }
        Ok(())
    }

    fn record_failure(&self, error: NewsError) -> NewsRefreshResult {
        if let Ok(mut state) = self.state.lock() {
            state.last_attempt_unix = now_unix_seconds();
            state.last_error = Some(error);
        }
        NewsRefreshResult::Failed(error)
    }

    fn verify_record(&self, record: &NewsCacheRecord) -> Result<(NewsFeed, u64), NewsError> {
        let signer = self
            .signer
            .lock()
            .ok()
            .and_then(|signer| signer.clone())
            .ok_or(NewsError::CertificateUnavailable)?;
        let framing = i2pr_su3::parse(
            &record.su3,
            i2pr_su3::Su3Limits {
                max_file_bytes: self.config.max_su3_bytes,
                max_content_bytes: MAX_NEWS_SU3_BYTES,
                max_signer_id_bytes: i2pr_su3::MAX_SIGNER_ID_BYTES,
                max_version_bytes: i2pr_su3::MAX_VERSION_BYTES,
            },
        )
        .map_err(|_| NewsError::UntrustedSignature)?;
        let now = now_unix_seconds();
        i2pr_su3::verify_rsa_sha512(&record.su3, &framing, &signer, now)
            .map_err(|_| NewsError::UntrustedSignature)?;
        if framing.content_type != NEWS_CONTENT_TYPE {
            return Err(NewsError::WrongContainerType);
        }
        let feed = parse_authenticated_content(
            framing.file_type,
            framing.content_type,
            framing
                .content(&record.su3)
                .map_err(|_| NewsError::InvalidCache)?,
        )?;
        Ok((feed, now))
    }

    fn commit_not_modified(
        &self,
        previous: Option<CachedNews>,
        response: FetchResponse,
    ) -> NewsRefreshResult {
        let Some(mut previous) = previous else {
            return self.record_failure(NewsError::InvalidCache);
        };
        if let Some(etag) = response.etag {
            previous.etag = Some(etag);
        }
        if let Some(last_modified) = response.last_modified {
            previous.last_modified = Some(last_modified);
        }
        let last_attempt_unix = now_unix_seconds();
        match encode_cache_record(
            &previous.su3,
            previous.etag.as_deref(),
            previous.last_modified.as_deref(),
        )
        .and_then(|bytes| {
            VerifiedContentCacheStore::in_data_dir(&self.data_dir)
                .publish(&bytes)
                .map_err(|_| NewsError::CacheUnavailable)
        }) {
            Ok(()) => {
                if let Ok(mut state) = self.state.lock() {
                    state.current = Some(previous);
                    state.last_attempt_unix = last_attempt_unix;
                    state.last_error = None;
                }
                NewsRefreshResult::NotModified
            }
            Err(error) => self.record_failure(error),
        }
    }

    fn commit_new_content(&self, response: FetchResponse) -> NewsRefreshResult {
        let record = NewsCacheRecord {
            su3: response.body,
            etag: response.etag,
            last_modified: response.last_modified,
        };
        let (feed, verified_at_unix) = match self.verify_record(&record) {
            Ok(verified) => verified,
            Err(error) => return self.record_failure(error),
        };
        let encoded = match encode_cache_record(
            &record.su3,
            record.etag.as_deref(),
            record.last_modified.as_deref(),
        ) {
            Ok(encoded) => encoded,
            Err(error) => return self.record_failure(error),
        };
        if VerifiedContentCacheStore::in_data_dir(&self.data_dir)
            .publish(&encoded)
            .is_err()
        {
            return self.record_failure(NewsError::CacheUnavailable);
        }
        let cached = CachedNews {
            su3: record.su3,
            etag: record.etag,
            last_modified: record.last_modified,
            feed,
            verified_at_unix,
        };
        if let Ok(mut state) = self.state.lock() {
            state.current = Some(cached);
            state.last_attempt_unix = verified_at_unix;
            state.last_error = None;
            NewsRefreshResult::Updated
        } else {
            NewsRefreshResult::Failed(NewsError::CacheUnavailable)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NewsCacheRecord {
    su3: Vec<u8>,
    etag: Option<String>,
    last_modified: Option<String>,
}

fn encode_cache_record(
    su3: &[u8],
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<Vec<u8>, NewsError> {
    let etag = etag.unwrap_or_default();
    let last_modified = last_modified.unwrap_or_default();
    for value in [etag, last_modified] {
        i2pr_addressbook::validate_subscription_validator(value)
            .map_err(|_| NewsError::InvalidCache)?;
    }
    if su3.is_empty()
        || su3.len() > MAX_NEWS_SU3_BYTES
        || etag.len() > u16::MAX as usize
        || last_modified.len() > u16::MAX as usize
    {
        return Err(NewsError::InvalidCache);
    }
    let mut record = Vec::with_capacity(14 + etag.len() + last_modified.len() + su3.len());
    record.extend_from_slice(CACHE_MAGIC);
    record.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    record.extend_from_slice(&(su3.len() as u32).to_le_bytes());
    record.extend_from_slice(&(etag.len() as u16).to_le_bytes());
    record.extend_from_slice(&(last_modified.len() as u16).to_le_bytes());
    record.extend_from_slice(etag.as_bytes());
    record.extend_from_slice(last_modified.as_bytes());
    record.extend_from_slice(su3);
    Ok(record)
}

fn decode_cache_record(bytes: &[u8], max_su3_bytes: usize) -> Result<NewsCacheRecord, NewsError> {
    if bytes.len() < 16 || bytes.get(..8) != Some(CACHE_MAGIC) {
        return Err(NewsError::InvalidCache);
    }
    if u16::from_le_bytes([bytes[8], bytes[9]]) != CACHE_VERSION {
        return Err(NewsError::InvalidCache);
    }
    let su3_len = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    let etag_len = u16::from_le_bytes([bytes[14], bytes[15]]) as usize;
    if bytes.len() < 18 {
        return Err(NewsError::InvalidCache);
    }
    let last_modified_len = u16::from_le_bytes([bytes[16], bytes[17]]) as usize;
    if su3_len == 0 || su3_len > max_su3_bytes {
        return Err(NewsError::InvalidCache);
    }
    let etag_start = 18_usize;
    let modified_start = etag_start
        .checked_add(etag_len)
        .ok_or(NewsError::InvalidCache)?;
    let su3_start = modified_start
        .checked_add(last_modified_len)
        .ok_or(NewsError::InvalidCache)?;
    let expected = su3_start
        .checked_add(su3_len)
        .ok_or(NewsError::InvalidCache)?;
    if expected != bytes.len() {
        return Err(NewsError::InvalidCache);
    }
    let etag = decode_validator(&bytes[etag_start..modified_start])?;
    let last_modified = decode_validator(&bytes[modified_start..su3_start])?;
    Ok(NewsCacheRecord {
        su3: bytes[su3_start..].to_vec(),
        etag,
        last_modified,
    })
}

fn decode_validator(bytes: &[u8]) -> Result<Option<String>, NewsError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let value = std::str::from_utf8(bytes).map_err(|_| NewsError::InvalidCache)?;
    i2pr_addressbook::validate_subscription_validator(value)
        .map_err(|_| NewsError::InvalidCache)?;
    Ok(Some(value.to_owned()))
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn parse_authenticated_content(
    file_type: u8,
    content_type: u8,
    content: &[u8],
) -> Result<NewsFeed, NewsError> {
    if content_type != NEWS_CONTENT_TYPE
        || !matches!(file_type, NEWS_FILE_TYPE_XML | NEWS_FILE_TYPE_GZIP_XML)
    {
        return Err(NewsError::WrongContainerType);
    }
    if content.len() > MAX_NEWS_SU3_BYTES {
        return Err(NewsError::CompressedTooLarge);
    }
    let xml = if file_type == NEWS_FILE_TYPE_GZIP_XML {
        let decoder = flate2::read::GzDecoder::new(content);
        let mut bounded = decoder.take((MAX_NEWS_XML_BYTES + 1) as u64);
        let mut expanded = Vec::with_capacity(content.len().min(MAX_NEWS_XML_BYTES));
        bounded
            .read_to_end(&mut expanded)
            .map_err(|_| NewsError::Decompression)?;
        if expanded.len() > MAX_NEWS_XML_BYTES {
            return Err(NewsError::XmlTooLarge);
        }
        expanded
    } else {
        if content.len() > MAX_NEWS_XML_BYTES {
            return Err(NewsError::XmlTooLarge);
        }
        content.to_vec()
    };
    let text = std::str::from_utf8(&xml).map_err(|_| NewsError::MalformedXml)?;
    parse_atom(text)
}

fn parse_atom(xml: &str) -> Result<NewsFeed, NewsError> {
    let mut reader = Reader::from_str(xml);
    let config = reader.config_mut();
    config.trim_text(false);
    config.check_end_names = true;
    config.check_comments = true;

    let mut stack: Vec<String> = Vec::with_capacity(8);
    let mut feed_namespace_valid = false;
    let mut feed_seen = false;
    let mut feed_updated = None;
    let mut entries = Vec::new();
    let mut current_entry: Option<NewsEntryBuilder> = None;
    let mut current_text = String::new();
    let mut total_text = 0_usize;

    loop {
        let event = reader.read_event().map_err(|_| NewsError::MalformedXml)?;
        match event {
            Event::Start(start) => {
                let raw_name = start.name();
                if raw_name.as_ref().contains(':') {
                    return Err(NewsError::InvalidAtom);
                }
                let name = raw_name.as_ref().to_owned();
                for attribute in start.attributes().with_checks(true) {
                    let attribute = attribute.map_err(|_| NewsError::MalformedXml)?;
                    if attribute.key.as_ref() == "xmlns"
                        && attribute.value.as_ref() != ATOM_NAMESPACE
                    {
                        return Err(NewsError::InvalidAtom);
                    }
                    if attribute.key.as_ref().contains(':')
                        && attribute.key.as_ref() != "xmlns"
                        && !attribute.key.as_ref().starts_with("xmlns:")
                    {
                        return Err(NewsError::InvalidAtom);
                    }
                }
                if stack.is_empty() {
                    if name != "feed" {
                        return Err(NewsError::InvalidAtom);
                    }
                    feed_namespace_valid = start.attributes().with_checks(true).any(|attribute| {
                        attribute.is_ok_and(|attribute| {
                            attribute.key.as_ref() == "xmlns"
                                && attribute.value.as_ref() == ATOM_NAMESPACE
                        })
                    });
                    if !feed_namespace_valid {
                        return Err(NewsError::InvalidAtom);
                    }
                    feed_seen = true;
                }
                push_tag(&mut stack, name)?;
            }
            Event::Empty(empty) => {
                let raw_name = empty.name();
                if raw_name.as_ref().contains(':') {
                    return Err(NewsError::InvalidAtom);
                }
                let name = raw_name.as_ref();
                for attribute in empty.attributes().with_checks(true) {
                    let attribute = attribute.map_err(|_| NewsError::MalformedXml)?;
                    if (attribute.key.as_ref() == "xmlns"
                        && attribute.value.as_ref() != ATOM_NAMESPACE)
                        || (attribute.key.as_ref().contains(':')
                            && attribute.key.as_ref() != "xmlns"
                            && !attribute.key.as_ref().starts_with("xmlns:"))
                    {
                        return Err(NewsError::InvalidAtom);
                    }
                }
                let parent = stack
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("/");
                if stack.is_empty()
                    || matches!(
                        (parent.as_str(), name),
                        ("feed", "entry" | "updated")
                            | ("feed/entry", "title" | "updated" | "summary" | "content")
                    )
                {
                    return Err(NewsError::InvalidAtom);
                }
            }
            Event::Text(text) => {
                let decoded = text.xml10_content();
                add_text_for_path(&stack, &mut current_text, decoded.as_ref(), &mut total_text)?;
            }
            Event::CData(data) => {
                let decoded = data.xml10_content();
                add_text_for_path(&stack, &mut current_text, decoded.as_ref(), &mut total_text)?;
            }
            Event::GeneralRef(reference) => {
                let value = if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|_| NewsError::MalformedXml)?
                {
                    character
                } else {
                    match reference.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "apos" => '\'',
                        "quot" => '"',
                        _ => return Err(NewsError::MalformedXml),
                    }
                };
                add_text_for_path(
                    &stack,
                    &mut current_text,
                    &value.to_string(),
                    &mut total_text,
                )?;
            }
            Event::End(end) => {
                let local_name = end.local_name();
                let name = local_name.as_ref();
                if stack.last().map(String::as_str) != Some(name) {
                    return Err(NewsError::MalformedXml);
                }
                let path = stack.join("/");
                if path == "feed/updated" {
                    let value = finish_text(&mut current_text);
                    validate_timestamp(&value)?;
                    if feed_updated.is_some() {
                        return Err(NewsError::InvalidAtom);
                    }
                    feed_updated = Some(value);
                } else if path == "feed/entry/title" {
                    let value = finish_text(&mut current_text);
                    let entry = current_entry.as_mut().ok_or(NewsError::InvalidAtom)?;
                    if entry.title.is_some() {
                        return Err(NewsError::InvalidAtom);
                    }
                    entry.title = Some(value);
                } else if path == "feed/entry/updated" {
                    let value = finish_text(&mut current_text);
                    validate_timestamp(&value)?;
                    let entry = current_entry.as_mut().ok_or(NewsError::InvalidAtom)?;
                    if entry.updated.is_some() {
                        return Err(NewsError::InvalidAtom);
                    }
                    entry.updated = Some(value);
                } else if matches!(path.as_str(), "feed/entry/summary" | "feed/entry/content") {
                    let value = finish_text(&mut current_text);
                    let entry = current_entry.as_mut().ok_or(NewsError::InvalidAtom)?;
                    if entry.text.replace(value).is_some() {
                        return Err(NewsError::InvalidAtom);
                    }
                } else if path == "feed/entry" {
                    let entry = current_entry
                        .take()
                        .ok_or(NewsError::InvalidAtom)?
                        .finish()?;
                    entries.push(entry);
                    if entries.len() > MAX_NEWS_ENTRIES {
                        return Err(NewsError::TooManyEntries);
                    }
                } else if path == "feed" && !feed_namespace_valid {
                    return Err(NewsError::InvalidAtom);
                }
                stack.pop();
            }
            Event::Decl(declaration) => {
                if declaration
                    .encoding()
                    .transpose()
                    .map_err(|_| NewsError::MalformedXml)?
                    .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("UTF-8"))
                {
                    return Err(NewsError::MalformedXml);
                }
            }
            Event::Comment(_) => {}
            Event::DocType(_) | Event::PI(_) => return Err(NewsError::MalformedXml),
            Event::Eof => break,
        }
        if stack.len() > MAX_NEWS_DEPTH {
            return Err(NewsError::TooDeep);
        }
        if stack == ["feed", "entry"] && current_entry.is_none() {
            current_entry = Some(NewsEntryBuilder::default());
            current_text.clear();
        }
    }
    if !feed_seen || !feed_namespace_valid || !stack.is_empty() {
        return Err(NewsError::InvalidAtom);
    }
    let updated = feed_updated.ok_or(NewsError::MissingField)?;
    let rendered = render_entries(&entries)?;
    Ok(NewsFeed {
        updated,
        entries,
        rendered,
    })
}

#[derive(Default)]
struct NewsEntryBuilder {
    updated: Option<String>,
    title: Option<String>,
    text: Option<String>,
}

impl NewsEntryBuilder {
    fn finish(self) -> Result<NewsEntry, NewsError> {
        let title = self.title.ok_or(NewsError::MissingField)?;
        if title.is_empty() || title.len() > MAX_NEWS_ENTRY_TEXT_BYTES {
            return Err(NewsError::TextTooLarge);
        }
        Ok(NewsEntry {
            updated: self.updated.ok_or(NewsError::MissingField)?,
            title,
            text: self.text.unwrap_or_default(),
        })
    }
}

fn push_tag(stack: &mut Vec<String>, name: String) -> Result<(), NewsError> {
    stack.push(name);
    if stack.len() > MAX_NEWS_DEPTH {
        return Err(NewsError::TooDeep);
    }
    Ok(())
}

fn add_text_for_path(
    stack: &[String],
    target: &mut String,
    value: &str,
    total: &mut usize,
) -> Result<(), NewsError> {
    *total = total
        .checked_add(value.len())
        .ok_or(NewsError::TextTooLarge)?;
    if *total > MAX_NEWS_TEXT_BYTES {
        return Err(NewsError::TextTooLarge);
    }
    let path = stack
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("/");
    let capture = matches!(
        path.as_str(),
        "feed/updated"
            | "feed/entry/title"
            | "feed/entry/updated"
            | "feed/entry/summary"
            | "feed/entry/content"
    ) || path.starts_with("feed/entry/summary/")
        || path.starts_with("feed/entry/content/");
    if !capture {
        return Ok(());
    }
    if target.len().saturating_add(value.len()) > MAX_NEWS_ENTRY_TEXT_BYTES {
        return Err(NewsError::TextTooLarge);
    }
    for character in value.chars() {
        if character == '<' || character == '>' {
            target.push(' ');
        } else if character == '\n'
            || character == '\r'
            || character == '\t'
            || !character.is_control()
        {
            target.push(character);
        } else {
            target.push(' ');
        }
    }
    Ok(())
}

fn finish_text(value: &mut String) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    value.clear();
    compact
}

fn render_entries(entries: &[NewsEntry]) -> Result<String, NewsError> {
    let mut rendered = String::new();
    for (index, entry) in entries.iter().enumerate() {
        if index != 0 {
            rendered.push('\n');
        }
        rendered.push_str(&entry.updated);
        rendered.push_str(" — ");
        rendered.push_str(&entry.title);
        if !entry.text.is_empty() {
            rendered.push('\n');
            rendered.push_str(&entry.text);
        }
        if rendered.len() > MAX_NEWS_TEXT_BYTES {
            return Err(NewsError::TextTooLarge);
        }
    }
    Ok(rendered)
}

fn validate_timestamp(value: &str) -> Result<(), NewsError> {
    // RFC 3339: full date/time, seconds, optional fractional seconds, and
    // either UTC or an explicit numeric offset.
    let (date_time, zone) = if let Some(prefix) = value.strip_suffix('Z') {
        (prefix, "Z")
    } else if let Some(index) = value.rfind(['+', '-']) {
        (&value[..index], &value[index..])
    } else {
        return Err(NewsError::InvalidTimestamp);
    };
    let (date, time) = date_time
        .split_once('T')
        .ok_or(NewsError::InvalidTimestamp)?;
    let mut date_parts = date.split('-');
    let year = date_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    let month = date_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    let day = date_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    if date_parts.next().is_some() || month == 0 || month > 12 || year == 0 {
        return Err(NewsError::InvalidTimestamp);
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > days {
        return Err(NewsError::InvalidTimestamp);
    }
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    if time.contains('.')
        && (fraction.is_empty()
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > 9)
    {
        return Err(NewsError::InvalidTimestamp);
    }
    let mut clock_parts = clock.split(':');
    let hour = clock_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    let minute = clock_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    let second = clock_parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(NewsError::InvalidTimestamp)?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return Err(NewsError::InvalidTimestamp);
    }
    if zone != "Z" {
        let (hours, minutes) = zone[1..]
            .split_once(':')
            .ok_or(NewsError::InvalidTimestamp)?;
        let offset_hour = hours
            .parse::<u32>()
            .map_err(|_| NewsError::InvalidTimestamp)?;
        let offset_minute = minutes
            .parse::<u32>()
            .map_err(|_| NewsError::InvalidTimestamp)?;
        if zone.len() != 6 || offset_hour > 23 || offset_minute > 59 {
            return Err(NewsError::InvalidTimestamp);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addressbook_fetch::FetchError;
    use sad_rsa::signature::{SignatureEncoding, Signer};
    use std::future::Future;
    use std::pin::Pin;

    const VALID_ATOM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <updated>2026-10-04T12:00:00Z</updated>
  <entry><title>Router &amp; network</title><updated>2026-10-04T11:59:00+00:00</updated>
    <summary>News <b>item</b>.</summary>
  </entry>
</feed>"#;

    struct SignedNewsIdentity {
        private_key: sad_rsa::RsaPrivateKey,
        certificate_der: Vec<u8>,
    }

    fn signed_news_identity() -> SignedNewsIdentity {
        use rsa_rand_core::SeedableRng;
        use sad_rsa::pkcs8::EncodePrivateKey;
        use sad_rsa::rand_core as rsa_rand_core;

        let mut rng = rand_chacha_10::ChaCha8Rng::seed_from_u64(0x170_322);
        let private_key = sad_rsa::RsaPrivateKey::new(&mut rng, 2048).expect("test RSA key");
        let pkcs8 = private_key.to_pkcs8_der().expect("PKCS8 key");
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_bytes());
        let key_pair =
            rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&key_der, &rcgen::PKCS_RSA_SHA512)
                .expect("rcgen reads RSA test key");
        let certificate = rcgen::CertificateParams::new(vec!["router-news".to_owned()])
            .expect("certificate params")
            .self_signed(&key_pair)
            .expect("self-signed test certificate");
        SignedNewsIdentity {
            private_key,
            certificate_der: certificate.der().to_vec(),
        }
    }

    fn signed_news_container(
        identity: &SignedNewsIdentity,
        signer_id: &str,
        content_type: u8,
        file_type: u8,
        content: &[u8],
    ) -> Vec<u8> {
        let signing_key = sad_rsa::pkcs1v15::SigningKey::<sad_rsa::sha2::Sha512>::new(
            identity.private_key.clone(),
        );
        // The fixture key is generated as RSA-2048.
        let signature_length = 256_usize;
        let mut signed = Vec::new();
        signed.extend_from_slice(b"I2Psu3");
        signed.extend_from_slice(&[0, 0]); // unused byte, format version
        signed.extend_from_slice(&6_u16.to_be_bytes());
        signed.extend_from_slice(&(signature_length as u16).to_be_bytes());
        signed.extend_from_slice(&[0, 16, 0, signer_id.len() as u8]);
        signed.extend_from_slice(&(content.len() as u64).to_be_bytes());
        signed.extend_from_slice(&[0, file_type, 0, content_type]);
        signed.extend_from_slice(&[0; 12]);
        signed.extend_from_slice(b"20261004\0\0\0\0\0\0\0\0");
        signed.extend_from_slice(signer_id.as_bytes());
        signed.extend_from_slice(content);
        let signature = signing_key.sign(&signed);
        signed.extend_from_slice(&signature.to_bytes());
        signed
    }

    #[test]
    fn parses_and_sanitizes_bounded_atom_feed() {
        let atom_with_empty_link = VALID_ATOM.replace(
            "<entry>",
            "<entry><link href=\"https://example.invalid/\" rel=\"alternate\" />",
        );
        let feed = parse_authenticated_content(
            NEWS_FILE_TYPE_XML,
            NEWS_CONTENT_TYPE,
            atom_with_empty_link.as_bytes(),
        )
        .unwrap();
        assert_eq!(feed.updated, "2026-10-04T12:00:00Z");
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0].title, "Router & network");
        assert_eq!(feed.entries[0].text, "News item.");
        assert!(feed.rendered.contains("Router & network"));
    }

    #[test]
    fn rejects_untrusted_markup_invalid_dates_and_wrong_su3_types() {
        let doctype = VALID_ATOM.replace(
            "<feed ",
            "<!DOCTYPE feed [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><feed ",
        );
        assert_eq!(
            parse_authenticated_content(NEWS_FILE_TYPE_XML, NEWS_CONTENT_TYPE, doctype.as_bytes()),
            Err(NewsError::MalformedXml)
        );
        let bad_date = VALID_ATOM.replace("2026-10-04T12:00:00Z", "2026-02-30T12:00:00Z");
        assert_eq!(
            parse_authenticated_content(NEWS_FILE_TYPE_XML, NEWS_CONTENT_TYPE, bad_date.as_bytes()),
            Err(NewsError::InvalidTimestamp)
        );
        assert_eq!(
            parse_authenticated_content(NEWS_FILE_TYPE_XML, 0, VALID_ATOM.as_bytes()),
            Err(NewsError::WrongContainerType)
        );
    }

    #[test]
    fn rejects_non_utf8_declarations_empty_required_elements_and_duplicates() {
        let latin1 = VALID_ATOM.replace("UTF-8", "ISO-8859-1");
        let empty_title = VALID_ATOM.replace("<title>Router &amp; network</title>", "<title/>");
        let duplicate_updated =
            VALID_ATOM.replace("</feed>", "<updated>2026-10-04T12:00:01Z</updated></feed>");
        for invalid in [latin1, empty_title, duplicate_updated] {
            assert!(
                parse_authenticated_content(
                    NEWS_FILE_TYPE_XML,
                    NEWS_CONTENT_TYPE,
                    invalid.as_bytes()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn feed_text_cannot_smuggle_html_markup_into_plain_rendering() {
        let markup = VALID_ATOM.replace(
            "Router &amp; network",
            "Router &lt;script&gt;alert(1)&lt;/script&gt;",
        );
        let feed =
            parse_authenticated_content(NEWS_FILE_TYPE_XML, NEWS_CONTENT_TYPE, markup.as_bytes())
                .unwrap();
        assert!(!feed.rendered.contains('<'));
        assert!(!feed.rendered.contains('>'));
    }

    #[test]
    fn rejects_gzip_expansion_over_the_xml_ceiling() {
        use std::io::Write;

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&vec![b'a'; MAX_NEWS_XML_BYTES + 1])
            .unwrap();
        let compressed = encoder.finish().unwrap();
        assert_eq!(
            parse_authenticated_content(NEWS_FILE_TYPE_GZIP_XML, NEWS_CONTENT_TYPE, &compressed,),
            Err(NewsError::XmlTooLarge)
        );
    }

    #[test]
    fn signed_news_cache_record_has_exact_bounds_and_conditional_validators() {
        let encoded = encode_cache_record(
            b"su3-container",
            Some("\"v1\""),
            Some("Wed, 21 Oct 2015 07:28:00 GMT"),
        )
        .unwrap();
        let decoded = decode_cache_record(&encoded, 1024).unwrap();
        assert_eq!(decoded.su3, b"su3-container");
        assert_eq!(decoded.etag.as_deref(), Some("\"v1\""));
        assert_eq!(
            decoded.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            decode_cache_record(&trailing, 1024),
            Err(NewsError::InvalidCache)
        );
        assert_eq!(
            encode_cache_record(b"su3-container", Some("bad\netag"), None),
            Err(NewsError::InvalidCache)
        );
    }

    struct FailedFetcher;

    impl BoundedContentFetcher for FailedFetcher {
        fn fetch<'a>(
            &'a self,
            _url: &'a str,
            _etag: Option<&'a str>,
            _last_modified: Option<&'a str>,
            _timeout: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>> {
            Box::pin(async { Err(FetchError::Unavailable) })
        }

        fn fetch_bounded<'a>(
            &'a self,
            url: &'a str,
            etag: Option<&'a str>,
            last_modified: Option<&'a str>,
            _max_body_bytes: usize,
            timeout: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>> {
            self.fetch(url, etag, last_modified, timeout)
        }
    }

    struct ResponseFetcher {
        response: Mutex<Option<FetchResponse>>,
        validators: Mutex<Vec<(Option<String>, Option<String>)>>,
    }

    impl ResponseFetcher {
        fn one(response: FetchResponse) -> Self {
            Self {
                response: Mutex::new(Some(response)),
                validators: Mutex::new(Vec::new()),
            }
        }
    }

    impl BoundedContentFetcher for ResponseFetcher {
        fn fetch<'a>(
            &'a self,
            url: &'a str,
            etag: Option<&'a str>,
            last_modified: Option<&'a str>,
            timeout: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>> {
            self.fetch_bounded(url, etag, last_modified, MAX_NEWS_SU3_BYTES, timeout)
        }

        fn fetch_bounded<'a>(
            &'a self,
            _url: &'a str,
            etag: Option<&'a str>,
            last_modified: Option<&'a str>,
            _max_body_bytes: usize,
            _timeout: Duration,
        ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>> {
            self.validators
                .lock()
                .unwrap()
                .push((etag.map(str::to_owned), last_modified.map(str::to_owned)));
            let response = self.response.lock().unwrap().take();
            Box::pin(async move { response.ok_or(FetchError::Unavailable) })
        }
    }

    fn news_test_config(_data_dir: &std::path::Path, certificate_path: PathBuf) -> NewsConfig {
        NewsConfig {
            enabled: true,
            source_url: Some("http://news.i2p/feed.su3".to_owned()),
            signer_id: Some("router-news".to_owned()),
            certificate_path: Some(certificate_path),
            proxy_host: "127.0.0.1".parse().unwrap(),
            proxy_port: 4444,
            max_su3_bytes: MAX_NEWS_SU3_BYTES,
            refresh_interval: Duration::from_secs(3600),
        }
    }

    fn write_test_certificate(directory: &std::path::Path, certificate: &[u8]) -> PathBuf {
        let path = directory.join("router-news.der");
        fs::write(&path, certificate).unwrap();
        path
    }

    fn response(status: u16, body: Vec<u8>, etag: Option<&str>) -> FetchResponse {
        FetchResponse {
            status,
            body,
            etag: etag.map(str::to_owned),
            last_modified: Some("Sun, 04 Oct 2026 12:00:00 GMT".to_owned()),
        }
    }

    #[tokio::test]
    async fn authenticated_news_verifies_before_parse_and_survives_304_restart() {
        let identity = signed_news_identity();
        let directory = tempfile::tempdir().unwrap();
        let certificate_path = write_test_certificate(directory.path(), &identity.certificate_der);
        let body = signed_news_container(
            &identity,
            "router-news",
            NEWS_CONTENT_TYPE,
            NEWS_FILE_TYPE_XML,
            VALID_ATOM.as_bytes(),
        );
        let frame = i2pr_su3::parse(&body, i2pr_su3::Su3Limits::default()).unwrap();
        let signer =
            i2pr_su3::rsa_signer_from_certificate("router-news", &identity.certificate_der)
                .unwrap();
        assert_eq!(
            frame.signed_bytes(&body).unwrap().len() + frame.signature(&body).unwrap().len(),
            body.len()
        );
        assert_eq!(frame.signature_length, signer.modulus.len());
        i2pr_su3::verify_rsa_sha512(&body, &frame, &signer, now_unix_seconds()).unwrap();
        let first_fetch = Arc::new(ResponseFetcher::one(response(
            200,
            body.clone(),
            Some("\"v1\""),
        )));
        let manager = NewsManager::new(
            news_test_config(directory.path(), certificate_path.clone()),
            directory.path().to_owned(),
            first_fetch,
        );
        assert_eq!(manager.refresh_once().await, NewsRefreshResult::Updated);
        let published = manager.snapshot(now_unix_seconds()).expect("verified feed");
        assert!(published.rendered.contains("Router & network"));

        let unchanged_fetch = Arc::new(ResponseFetcher::one(response(304, Vec::new(), None)));
        let restarted = NewsManager::new(
            news_test_config(directory.path(), certificate_path),
            directory.path().to_owned(),
            unchanged_fetch.clone(),
        );
        assert_eq!(
            restarted.refresh_once().await,
            NewsRefreshResult::NotModified
        );
        assert_eq!(
            unchanged_fetch.validators.lock().unwrap()[0].0.as_deref(),
            Some("\"v1\"")
        );
        let after_restart = restarted
            .snapshot(now_unix_seconds())
            .expect("cached signature reverified on restart");
        assert_eq!(after_restart.rendered, published.rendered);

        let mut invalid_update = body;
        *invalid_update.last_mut().unwrap() ^= 1;
        let failed_refresh = NewsManager::new(
            news_test_config(directory.path(), directory.path().join("router-news.der")),
            directory.path().to_owned(),
            Arc::new(ResponseFetcher::one(response(
                200,
                invalid_update,
                Some("\"v2\""),
            ))),
        );
        assert_eq!(
            failed_refresh.refresh_once().await,
            NewsRefreshResult::Failed(NewsError::UntrustedSignature)
        );
        assert_eq!(
            failed_refresh
                .snapshot(now_unix_seconds())
                .expect("prior verified feed retained")
                .rendered,
            published.rendered
        );
    }

    #[tokio::test]
    async fn authenticated_news_accepts_signed_gzip_feed() {
        use std::io::Write;

        let identity = signed_news_identity();
        let directory = tempfile::tempdir().unwrap();
        let certificate_path = write_test_certificate(directory.path(), &identity.certificate_der);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(VALID_ATOM.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let body = signed_news_container(
            &identity,
            "router-news",
            NEWS_CONTENT_TYPE,
            NEWS_FILE_TYPE_GZIP_XML,
            &compressed,
        );
        let manager = NewsManager::new(
            news_test_config(directory.path(), certificate_path),
            directory.path().to_owned(),
            Arc::new(ResponseFetcher::one(response(
                200,
                body,
                Some("\"gzip-v1\""),
            ))),
        );

        assert_eq!(manager.refresh_once().await, NewsRefreshResult::Updated);
        assert!(
            manager
                .snapshot(now_unix_seconds())
                .expect("signed gzip feed is published")
                .rendered
                .contains("Router & network")
        );
    }

    #[tokio::test]
    async fn authenticated_news_rejects_wrong_signer_signature_and_file_type() {
        let identity = signed_news_identity();
        let directory = tempfile::tempdir().unwrap();
        let certificate_path = write_test_certificate(directory.path(), &identity.certificate_der);
        let valid = signed_news_container(
            &identity,
            "router-news",
            NEWS_CONTENT_TYPE,
            NEWS_FILE_TYPE_XML,
            VALID_ATOM.as_bytes(),
        );
        let wrong_signer = signed_news_container(
            &identity,
            "attacker",
            NEWS_CONTENT_TYPE,
            NEWS_FILE_TYPE_XML,
            VALID_ATOM.as_bytes(),
        );
        let wrong_type = signed_news_container(
            &identity,
            "router-news",
            NEWS_CONTENT_TYPE,
            2,
            VALID_ATOM.as_bytes(),
        );
        let wrong_content_type = signed_news_container(
            &identity,
            "router-news",
            3,
            NEWS_FILE_TYPE_XML,
            VALID_ATOM.as_bytes(),
        );
        let mut invalid_signature = valid;
        *invalid_signature.last_mut().unwrap() ^= 1;
        for body in [
            wrong_signer,
            invalid_signature,
            wrong_type,
            wrong_content_type,
        ] {
            let manager = NewsManager::new(
                news_test_config(directory.path(), certificate_path.clone()),
                directory.path().to_owned(),
                Arc::new(ResponseFetcher::one(response(200, body, None))),
            );
            assert!(matches!(
                manager.refresh_once().await,
                NewsRefreshResult::Failed(
                    NewsError::UntrustedSignature | NewsError::WrongContainerType
                )
            ));
            assert!(manager.snapshot(now_unix_seconds()).is_none());
        }
    }

    #[tokio::test]
    async fn authenticated_news_rejects_expired_pinned_certificate() {
        let identity = signed_news_identity();
        let mut expired_certificate = identity.certificate_der.clone();
        let current_not_after = b"40960101000000Z";
        let expiry_offset = expired_certificate
            .windows(current_not_after.len())
            .position(|window| window == current_not_after)
            .expect("test certificate has generalized-time expiry");
        expired_certificate[expiry_offset..expiry_offset + current_not_after.len()]
            .copy_from_slice(b"20200101000000Z");

        let directory = tempfile::tempdir().unwrap();
        let certificate_path = write_test_certificate(directory.path(), &expired_certificate);
        let body = signed_news_container(
            &identity,
            "router-news",
            NEWS_CONTENT_TYPE,
            NEWS_FILE_TYPE_XML,
            VALID_ATOM.as_bytes(),
        );
        let manager = NewsManager::new(
            news_test_config(directory.path(), certificate_path),
            directory.path().to_owned(),
            Arc::new(ResponseFetcher::one(response(200, body, None))),
        );
        assert_eq!(
            manager.refresh_once().await,
            NewsRefreshResult::Failed(NewsError::UntrustedSignature)
        );
        assert!(manager.snapshot(now_unix_seconds()).is_none());
    }

    #[tokio::test]
    async fn failed_conditional_refresh_preserves_last_verified_news() {
        let directory = tempfile::tempdir().unwrap();
        let config = NewsConfig {
            enabled: true,
            source_url: Some("http://news.i2p/feed.su3".to_owned()),
            signer_id: Some("router-news".to_owned()),
            certificate_path: Some(directory.path().join("router.der")),
            proxy_host: "127.0.0.1".parse().unwrap(),
            proxy_port: 4444,
            max_su3_bytes: MAX_NEWS_SU3_BYTES,
            refresh_interval: Duration::from_secs(3600),
        };
        let manager =
            NewsManager::new(config, directory.path().to_owned(), Arc::new(FailedFetcher));
        *manager.initialized.lock().unwrap() = true;
        *manager.state.lock().unwrap() = NewsState {
            current: Some(CachedNews {
                su3: b"previous signed container".to_vec(),
                etag: Some("\"old\"".to_owned()),
                last_modified: None,
                feed: NewsFeed {
                    updated: "2026-10-04T12:00:00Z".to_owned(),
                    entries: Vec::new(),
                    rendered: "last verified item".to_owned(),
                },
                verified_at_unix: 100,
            }),
            last_attempt_unix: 100,
            last_error: None,
        };
        assert_eq!(
            manager.refresh_once().await,
            NewsRefreshResult::Failed(NewsError::FetchUnavailable)
        );
        let snapshot = manager.snapshot(101).expect("old feed remains published");
        assert_eq!(snapshot.rendered, "last verified item");
        assert_eq!(snapshot.last_error, Some(NewsError::FetchUnavailable));
        assert!(snapshot.last_attempt_unix >= 100);
    }
}
