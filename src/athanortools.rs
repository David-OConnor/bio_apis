//! Client for athanortools.com's JSON API: queue a tool run, follow it, and download its results.
//! https://athanortools.com/api_docs
//!
//! A run is a POST of the tool's form fields to `/api/<slug>/`, which answers at once with a job to
//! poll. Without an API token, the job belongs to the session cookie the submission sets, so the
//! same [`Client`] must poll and download it.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;
use ureq::{Agent, http::Response};

pub const BASE_URL: &str = "https://athanortools.com";

/// Polls and submissions. Generous, as a submission carries its input files.
const API_TIMEOUT: u64 = 120;
/// Raw-result archives of structure predictions can be hundreds of MB.
const DOWNLOAD_TIMEOUT: u64 = 2 * 60 * 60;
/// Job JSON includes the tool's own result object, which may hold logs.
const MAX_JSON: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    /// Whether the job is done, however it ended.
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// One job, as the API last reported it. URLs are absolute.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    /// The tool's slug.
    pub tool: String,
    pub status: JobStatus,
    /// Jobs ahead of this one; queued jobs only.
    pub position: Option<u64>,
    /// Aborted while executing; the result will be discarded.
    pub cancel_requested: bool,
    /// The page a person reads the run on.
    pub results_url: Option<String>,
    pub raw_results_url: Option<String>,
    /// The tool's result object; succeeded jobs only.
    pub result: Option<Value>,
    /// Failed jobs only.
    pub error: Option<ApiError>,
}

pub struct Client {
    base_url: String,
    token: Option<String>,
    /// Sent as `X-Bio-Web-Source`, which the site records each run under.
    source: Option<String>,
    /// `name=value` pairs from `Set-Cookie`.
    cookies: Vec<(String, String)>,
}

impl Client {
    /// `token` is an API token from the athanortools.com dashboard. Without one, runs are
    /// anonymous, and limited in number per day.
    pub fn new(token: Option<String>) -> Self {
        Self {
            base_url: BASE_URL.to_owned(),
            token: token.filter(|token| !token.trim().is_empty()),
            source: None,
            cookies: Vec::new(),
        }
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.trim_end_matches('/').to_owned();
        self
    }

    /// E.g. "molchanica".
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_owned());
        self
    }

    /// The URL of a path the API returned, e.g. `/results/<id>/`.
    pub fn absolute(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}{path}", self.base_url)
        }
    }

    /// Queue a run of the tool `slug`. `payload` is a JSON object of its form fields.
    pub fn submit(&mut self, slug: &str, payload: &Value) -> io::Result<Job> {
        let body = serde_json::to_string(payload)?;
        let url = format!("{}/api/{slug}/", self.base_url);

        let request = self
            .request(agent(API_TIMEOUT).post(&url))
            .header("Content-Type", "application/json");
        let mut response = request.send(body.as_bytes()).map_err(http_error)?;

        if response.status().as_u16() == 404 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("athanortools.com does not offer {slug}"),
            ));
        }
        self.store_cookies(&response);

        self.job_from(&mut response)
    }

    /// The job as it stands now.
    pub fn status(&self, job_id: &str) -> io::Result<Job> {
        let url = format!("{}/api/jobs/{job_id}/", self.base_url);
        let mut response = self
            .request(agent(API_TIMEOUT).get(&url))
            .call()
            .map_err(http_error)?;

        self.job_from(&mut response)
    }

    /// Give up on a queued or running job. One already running is abandoned rather than killed;
    /// its result is discarded.
    pub fn abort(&self, job_id: &str) -> io::Result<Job> {
        let url = format!("{}/api/jobs/{job_id}/abort/", self.base_url);
        let mut response = self
            .request(agent(API_TIMEOUT).post(&url))
            .send_empty()
            .map_err(http_error)?;

        self.job_from(&mut response)
    }

    /// Download a succeeded job's raw results, a ZIP of its run-log directories, into `dest`.
    /// Returns the number of bytes written.
    pub fn download_raw_results(&self, job: &Job, dest: &mut impl Write) -> io::Result<u64> {
        let url = job
            .raw_results_url
            .as_deref()
            .ok_or_else(|| io::Error::other("This job has no raw results to download"))?;

        let mut response = self
            .request(agent(DOWNLOAD_TIMEOUT).get(url))
            .call()
            .map_err(http_error)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = read_json(&mut response)
                .ok()
                .and_then(|json| api_error(&json))
                .map(|error| error.message)
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(io::Error::other(format!(
                "Downloading results failed: {message}"
            )));
        }

        io::copy(&mut response.body_mut().as_reader(), dest)
    }

    fn request<B>(&self, mut request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        request = request.header("Accept", "application/json");

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token.trim()));
        }
        if let Some(source) = &self.source {
            request = request.header("X-Bio-Web-Source", source);
        }
        if !self.cookies.is_empty() {
            let cookies: Vec<String> = self
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect();
            request = request.header("Cookie", cookies.join("; "));
        }

        request
    }

    fn store_cookies(&mut self, response: &Response<ureq::Body>) {
        for header in response.headers().get_all("set-cookie") {
            let Ok(text) = header.to_str() else {
                continue;
            };
            let pair = text.split(';').next().unwrap_or_default();
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let (name, value) = (name.trim().to_owned(), value.trim().to_owned());

            self.cookies.retain(|(existing, _)| *existing != name);
            if !value.is_empty() {
                self.cookies.push((name, value));
            }
        }
    }

    /// The job in a `{"ok": true, "job": ...}` response, or the error in any other.
    fn job_from(&self, response: &mut Response<ureq::Body>) -> io::Result<Job> {
        let status = response.status().as_u16();
        let json = read_json(response).map_err(|error| {
            io::Error::other(format!("athanortools.com: HTTP {status}: {error}"))
        })?;

        if json["ok"] != true {
            let error = api_error(&json).unwrap_or(ApiError {
                code: "unknown".into(),
                message: format!("HTTP {status}"),
            });
            let kind = match error.code.as_str() {
                "unauthorized" => io::ErrorKind::PermissionDenied,
                "not_found" => io::ErrorKind::NotFound,
                _ => io::ErrorKind::Other,
            };
            return Err(io::Error::new(
                kind,
                format!("athanortools.com: {}", error.message),
            ));
        }

        self.parse_job(&json["job"])
    }

    fn parse_job(&self, job: &Value) -> io::Result<Job> {
        let text = |key: &str| job[key].as_str().map(str::to_owned);
        let url = |key: &str| job[key].as_str().map(|path| self.absolute(path));

        let id = text("id").ok_or_else(|| io::Error::other("The job has no ID"))?;
        let status = job["status"]
            .as_str()
            .and_then(JobStatus::parse)
            .ok_or_else(|| io::Error::other("The job has an unknown status"))?;

        Ok(Job {
            id,
            tool: text("tool").unwrap_or_default(),
            status,
            position: job["position"].as_u64(),
            cancel_requested: job["cancel_requested"] == true,
            results_url: url("results_url"),
            raw_results_url: url("raw_results_url"),
            result: job.get("result").cloned(),
            error: api_error(job),
        })
    }
}

/// Unpack a raw-results archive into `dest`. Returns the files written. Entries that would land
/// outside `dest` are skipped.
pub fn extract_raw_results(archive: &Path, dest: &Path) -> io::Result<Vec<PathBuf>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?).map_err(io::Error::other)?;
    let mut written = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(io::Error::other)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let path = dest.join(relative);

        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        io::copy(&mut entry, &mut fs::File::create(&path)?)?;
        written.push(path);
    }

    Ok(written)
}

fn agent(timeout: u64) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn http_error(error: ureq::Error) -> io::Error {
    io::Error::other(format!("athanortools.com: {error}"))
}

fn read_json(response: &mut Response<ureq::Body>) -> io::Result<Value> {
    let mut text = String::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_JSON)
        .read_to_string(&mut text)?;

    serde_json::from_str(&text).map_err(io::Error::other)
}

fn api_error(json: &Value) -> Option<ApiError> {
    let error = json.get("error")?;

    Some(ApiError {
        code: error["code"].as_str().unwrap_or("unknown").to_owned(),
        message: error["message"]
            .as_str()
            .unwrap_or("Unknown error")
            .to_owned(),
    })
}
