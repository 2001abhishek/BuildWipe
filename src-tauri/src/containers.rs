//! Docker and Podman: unused images, stopped containers and build cache.
//!
//! Everything goes through the engine's own CLI. Their data lives in the
//! engine's storage, usually inside a virtual disk on Windows and macOS, so its
//! files are never touched directly. Removal never forces anything: the engine
//! itself refuses to remove an image a container uses or a container that runs.

use crate::platform::{native, Policy};
use crate::rules::Category;
use crate::scanner::Item;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Container items have ids rather than paths, starting with this.
pub const ID_PREFIX: &str = "container://";

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const REMOVE_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Docker,
    Podman,
}

impl Engine {
    const ALL: [Engine; 2] = [Engine::Docker, Engine::Podman];

    fn name(self) -> &'static str {
        match self {
            Engine::Docker => "Docker",
            Engine::Podman => "Podman",
        }
    }

    fn program(self) -> &'static str {
        match self {
            Engine::Docker => "docker",
            Engine::Podman => "podman",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// By id, or by every tag when it has several, since removing an image by
    /// id with several tags needs force.
    RemoveImage(Vec<String>),
    RemoveContainer(String),
    PruneBuildCache,
}

/// What the wipe will ask the engine to do. Kept in the session, so the UI only ever sends an id.
#[derive(Clone, Debug)]
pub struct Target {
    pub program: PathBuf,
    pub engine: Engine,
    pub action: Action,
    pub size: u64,
}

/// One removable thing, as read from the engine.
#[derive(Debug, PartialEq)]
struct Candidate {
    key: String,
    label: String,
    detail: String,
    size: u64,
    modified: u64,
    action: Action,
}

enum Unavailable {
    /// The CLI exists but the engine didn't answer.
    NotRunning,
    /// `docker` is Podman under another name, which Podman's own pass covers.
    Alias,
    Failed(String),
}

/// Lists what each installed engine could remove. Engines that aren't
/// installed are skipped quietly. One that is installed but not running is
/// reported through `notice`.
pub fn scan(
    policy: &Policy,
    cancel: &AtomicBool,
    mut found: impl FnMut(Item, Target),
    mut notice: impl FnMut(String),
) {
    for engine in Engine::ALL {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let Some(program) = find_program(engine.program(), &policy.container_cli_dirs()) else {
            continue;
        };
        match list(engine, &program) {
            Ok((version, candidates)) => {
                let group = format!("{} {version}", engine.name());
                for candidate in candidates {
                    let id = format!("{ID_PREFIX}{}/{}", engine.program(), candidate.key);
                    let target = Target {
                        program: program.clone(),
                        engine,
                        action: candidate.action,
                        size: candidate.size,
                    };
                    let item = Item {
                        path: id,
                        name: candidate.label.clone(),
                        category: Category::Container,
                        project_name: engine.name().to_string(),
                        project_path: group.clone(),
                        size: candidate.size,
                        files: 0,
                        modified: candidate.modified,
                        mount: String::new(),
                        note: None,
                        label: Some(candidate.label),
                        detail: Some(candidate.detail),
                    };
                    found(item, target);
                }
            }
            Err(Unavailable::Alias) => {}
            Err(Unavailable::NotRunning) => notice(format!(
                "{} is installed but not running, so its images and caches weren't checked. Start it and scan again.",
                engine.name()
            )),
            Err(Unavailable::Failed(reason)) => {
                notice(format!("Couldn't read {}'s images: {reason}", engine.name()))
            }
        }
    }
}

fn list(engine: Engine, program: &Path) -> Result<(String, Vec<Candidate>), Unavailable> {
    match engine {
        Engine::Docker => {
            let client = run(program, &["--version"], CHECK_TIMEOUT).map_err(Unavailable::Failed)?;
            if client.to_lowercase().contains("podman") {
                return Err(Unavailable::Alias);
            }
            let version = run(program, &["version", "--format", "{{.Server.Version}}"], CHECK_TIMEOUT)
                .map_err(|_| Unavailable::NotRunning)?;
            let usage = run(program, &["system", "df", "--verbose", "--format", "{{json .}}"], LIST_TIMEOUT)
                .map_err(Unavailable::Failed)?;
            let candidates = parse_docker(&usage).map_err(Unavailable::Failed)?;
            Ok((version.trim().to_string(), candidates))
        }
        Engine::Podman => {
            let version = run(program, &["version", "--format", "{{.Client.Version}}"], CHECK_TIMEOUT)
                .map_err(Unavailable::Failed)?;
            // On Windows and macOS this fails while the Podman machine is stopped.
            let images = run(program, &["images", "--format", "json"], LIST_TIMEOUT)
                .map_err(|_| Unavailable::NotRunning)?;
            let containers = run(program, &["ps", "--all", "--size", "--format", "json"], LIST_TIMEOUT)
                .map_err(Unavailable::Failed)?;
            let mut candidates = parse_podman_images(&images).map_err(Unavailable::Failed)?;
            candidates.extend(parse_podman_containers(&containers).map_err(Unavailable::Failed)?);
            Ok((version.trim().to_string(), candidates))
        }
    }
}

/// Removes one item. Returns the space freed, as measured by the scan.
pub fn remove(target: &Target) -> Result<u64, String> {
    let mut args: Vec<&str> = Vec::new();
    match &target.action {
        Action::RemoveImage(refs) => {
            args.extend(["image", "rm"]);
            args.extend(refs.iter().map(String::as_str));
        }
        Action::RemoveContainer(id) => args.extend(["container", "rm", id.as_str()]),
        Action::PruneBuildCache => args.extend(["builder", "prune", "--all", "--force"]),
    }
    match run(&target.program, &args, REMOVE_TIMEOUT) {
        Ok(_) => Ok(target.size),
        Err(error) if is_already_gone(&error) => Ok(0),
        Err(error) => Err(explain(target.engine, &error)),
    }
}

fn is_already_gone(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("no such image") || lower.contains("no such container") || lower.contains("image not known")
}

/// The engine's refusal, in plain words.
fn explain(engine: Engine, error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("is using") || lower.contains("being used") || lower.contains("in use") {
        "A container started using it after the scan, so it was left alone.".to_string()
    } else if lower.contains("running") && !lower.contains("daemon running") {
        "It's running now, so it was left alone.".to_string()
    } else if lower.contains("cannot connect") || lower.contains("daemon running") || lower.contains("connection refused") {
        format!("{} stopped running. Start it and try again.", engine.name())
    } else {
        error.to_string()
    }
}

/// The program's full path: from PATH first, then the OS's usual install folders.
fn find_program(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    on_path
        .iter()
        .chain(extra_dirs)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

/// Runs a CLI and returns its output, or the first line of its error.
fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = native::command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    // Read both pipes while waiting, so a large listing can't fill one and stall.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stderr.read_to_end(&mut text);
        text
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("It took too long to answer.".to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    };

    let stdout = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    if status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
    Err(stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("It stopped with {status}.")))
}

// ---- Docker -------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerUsage {
    #[serde(default)]
    images: Vec<DockerImage>,
    #[serde(default)]
    containers: Vec<DockerContainer>,
    #[serde(default)]
    build_cache: Vec<DockerCache>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerImage {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    containers: Value,
    #[serde(default)]
    size: String,
    #[serde(default)]
    unique_size: String,
    #[serde(default)]
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerContainer {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(default)]
    names: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerCache {
    #[serde(default)]
    in_use: Value,
    #[serde(default)]
    shared: Value,
    #[serde(default)]
    size: String,
    #[serde(default)]
    last_used_at: String,
    #[serde(default)]
    created_at: String,
}

const STOPPED: &[&str] = &["exited", "created", "dead", "stopped"];

fn parse_docker(output: &str) -> Result<Vec<Candidate>, String> {
    let usage: DockerUsage = serde_json::from_str(output.trim())
        .map_err(|e| format!("its disk usage report wasn't readable ({e})."))?;
    let mut candidates = Vec::new();

    // One row per tag, so tags of the same image are gathered first.
    let mut images: BTreeMap<&str, Vec<&DockerImage>> = BTreeMap::new();
    for image in &usage.images {
        images.entry(image.id.as_str()).or_default().push(image);
    }
    for (id, rows) in images {
        if id.is_empty() || !rows.iter().all(|row| count(&row.containers) == Some(0)) {
            continue;
        }
        let size = rows
            .iter()
            .filter_map(|row| parse_size(&row.unique_size).or_else(|| parse_size(&row.size)))
            .max()
            .unwrap_or(0);
        if size == 0 {
            continue;
        }
        let tags: Vec<String> = rows
            .iter()
            .filter(|row| row.repository != "<none>" && row.tag != "<none>" && !row.repository.is_empty())
            .map(|row| format!("{}:{}", row.repository, row.tag))
            .collect();
        let (label, detail) = match tags.first() {
            Some(first) => (first.clone(), "Unused image. No container uses it.".to_string()),
            None => (format!("Untagged image {}", short_id(id)), "Left behind by a rebuild or pull.".to_string()),
        };
        let refs = if tags.len() > 1 { tags } else { vec![id.to_string()] };
        candidates.push(Candidate {
            key: format!("image/{id}"),
            label,
            detail,
            size,
            modified: rows.iter().filter_map(|row| parse_time(&row.created_at)).max().unwrap_or(0),
            action: Action::RemoveImage(refs),
        });
    }

    for container in &usage.containers {
        if container.id.is_empty() || !STOPPED.contains(&container.state.to_lowercase().as_str()) {
            continue;
        }
        let name = container.names.split(',').next().unwrap_or_default();
        let detail = if container.state.eq_ignore_ascii_case("created") {
            format!("Created but never started, from {}", container.image)
        } else {
            format!("Stopped container, from {}", container.image)
        };
        candidates.push(Candidate {
            key: format!("container/{}", container.id),
            label: if name.is_empty() { short_id(&container.id).to_string() } else { name.to_string() },
            detail,
            size: parse_size(&container.size).unwrap_or(0),
            modified: parse_time(&container.created_at).unwrap_or(0),
            action: Action::RemoveContainer(container.id.clone()),
        });
    }

    // The cache that `docker system df` calls reclaimable: not in use, and not shared with an image.
    let unused: Vec<&DockerCache> = usage
        .build_cache
        .iter()
        .filter(|cache| !truthy(&cache.in_use) && !truthy(&cache.shared))
        .collect();
    let size: u64 = unused.iter().filter_map(|cache| parse_size(&cache.size)).sum();
    if size > 0 {
        candidates.push(Candidate {
            key: "build-cache".to_string(),
            label: "Build cache".to_string(),
            detail: format!("{} unused layers saved by docker build", unused.len()),
            size,
            modified: unused
                .iter()
                .filter_map(|cache| parse_time(&cache.last_used_at).or_else(|| parse_time(&cache.created_at)))
                .max()
                .unwrap_or(0),
            action: Action::PruneBuildCache,
        });
    }
    Ok(candidates)
}

// ---- Podman -------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PodmanImage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    names: Option<Vec<String>>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    containers: u64,
    #[serde(default)]
    created: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PodmanContainer {
    #[serde(default)]
    id: String,
    #[serde(default)]
    names: Option<Vec<String>>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    size: Option<PodmanSize>,
    #[serde(default)]
    created: i64,
    #[serde(default)]
    is_infra: bool,
    #[serde(default)]
    pod: String,
}

#[derive(Deserialize)]
struct PodmanSize {
    #[serde(rename = "rwSize", default)]
    rw_size: u64,
}

fn parse_podman_images(output: &str) -> Result<Vec<Candidate>, String> {
    let images: Vec<PodmanImage> = parse_json_list(output, "image list")?;
    Ok(images
        .into_iter()
        .filter(|image| !image.id.is_empty() && image.containers == 0 && image.size > 0)
        .map(|image| {
            let tags: Vec<String> = image
                .names
                .unwrap_or_default()
                .into_iter()
                .filter(|name| !name.contains("<none>"))
                .collect();
            let (label, detail) = match tags.first() {
                Some(first) => (first.clone(), "Unused image. No container uses it.".to_string()),
                None => (format!("Untagged image {}", short_id(&image.id)), "Left behind by a rebuild or pull.".to_string()),
            };
            let refs = if tags.len() > 1 { tags } else { vec![image.id.clone()] };
            Candidate {
                key: format!("image/{}", image.id),
                label,
                detail,
                size: image.size,
                modified: seconds_to_ms(image.created),
                action: Action::RemoveImage(refs),
            }
        })
        .collect())
}

fn parse_podman_containers(output: &str) -> Result<Vec<Candidate>, String> {
    let containers: Vec<PodmanContainer> = parse_json_list(output, "container list")?;
    Ok(containers
        .into_iter()
        // A pod's containers come and go with the pod, so they're left to it.
        .filter(|c| !c.id.is_empty() && !c.is_infra && c.pod.is_empty())
        .filter(|c| STOPPED.contains(&c.state.to_lowercase().as_str()))
        .map(|c| {
            let name = c.names.as_ref().and_then(|names| names.first()).cloned();
            let detail = if c.state.eq_ignore_ascii_case("created") {
                format!("Created but never started, from {}", c.image)
            } else {
                format!("Stopped container, from {}", c.image)
            };
            Candidate {
                key: format!("container/{}", c.id),
                label: name.unwrap_or_else(|| short_id(&c.id).to_string()),
                detail,
                size: c.size.map_or(0, |s| s.rw_size),
                modified: seconds_to_ms(c.created),
                action: Action::RemoveContainer(c.id.clone()),
            }
        })
        .collect())
}

/// Podman prints `null` rather than `[]` for an empty list.
fn parse_json_list<T: for<'de> Deserialize<'de>>(output: &str, what: &str) -> Result<Vec<T>, String> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("its {what} wasn't readable ({e})."))
}

// ---- Shared parsing -----------------------------------------------------

fn short_id(id: &str) -> &str {
    let bare = id.strip_prefix("sha256:").unwrap_or(id);
    &bare[..bare.len().min(12)]
}

fn count(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn seconds_to_ms(seconds: i64) -> u64 {
    u64::try_from(seconds).map_or(0, |s| s * 1000)
}

/// Sizes as Docker prints them: `1.237GB`, `20.7kB`, `0B`, or `4.1kB (virtual 915MB)`.
fn parse_size(text: &str) -> Option<u64> {
    let first = text.split_whitespace().next()?;
    let split = first.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = first.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale: f64 = match unit.to_lowercase().as_str() {
        "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "pb" => 1e15,
        "kib" => 1024.0,
        "mib" => 1024f64.powi(2),
        "gib" => 1024f64.powi(3),
        "tib" => 1024f64.powi(4),
        _ => return None,
    };
    Some((number * scale).round() as u64)
}

/// Times as Docker prints them: `2026-04-07 08:01:40 +0530 IST`, in Unix milliseconds.
fn parse_time(text: &str) -> Option<u64> {
    let mut parts = text.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let offset = parts.next().unwrap_or("+0000");

    let mut ymd = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (ymd.next()??, ymd.next()??, ymd.next()??);
    let mut hms = time.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: f64 = hms.next()?.parse().ok()?;

    let sign = match offset.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits = &offset[1..];
    if digits.len() != 4 {
        return None;
    }
    let offset_minutes = sign * (digits[..2].parse::<i64>().ok()? * 60 + digits[2..].parse::<i64>().ok()?);

    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 - offset_minutes * 60;
    let ms = seconds * 1000 + (second * 1000.0) as i64;
    u64::try_from(ms).ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from real `docker system df --verbose --format "{{json .}}"` output.
    const DOCKER: &str = r#"{
      "Images": [
        {"Containers":"0","CreatedAt":"2026-04-07 08:01:40 +0530 IST","ID":"sha256:cf09adf8c3eb","Repository":"rust","Tag":"1.94-slim","Size":"1.24GB","UniqueSize":"1.237GB"},
        {"Containers":"1","CreatedAt":"2026-01-01 00:00:00 +0000 UTC","ID":"sha256:inuse0000000","Repository":"postgres","Tag":"16","Size":"400MB","UniqueSize":"400MB"},
        {"Containers":"0","CreatedAt":"2026-02-01 00:00:00 +0000 UTC","ID":"sha256:dangling0000a","Repository":"<none>","Tag":"<none>","Size":"50MB","UniqueSize":"50MB"},
        {"Containers":"0","CreatedAt":"2026-03-01 00:00:00 +0000 UTC","ID":"sha256:twotags00000","Repository":"app","Tag":"latest","Size":"10MB","UniqueSize":"10MB"},
        {"Containers":"0","CreatedAt":"2026-03-01 00:00:00 +0000 UTC","ID":"sha256:twotags00000","Repository":"app","Tag":"v2","Size":"10MB","UniqueSize":"10MB"},
        {"Containers":"0","CreatedAt":"2026-03-01 00:00:00 +0000 UTC","ID":"sha256:shared000000","Repository":"base","Tag":"1","Size":"300MB","UniqueSize":"0B"}
      ],
      "Containers": [
        {"ID":"f912c22323b1","Image":"rust:1.94-slim","Names":"buildwipe-probe","State":"created","Size":"4.1kB","CreatedAt":"2026-11-15 13:50:26 +0530 IST"},
        {"ID":"aaaa11112222","Image":"postgres:16","Names":"db","State":"running","Size":"2MB","CreatedAt":"2026-11-01 10:00:00 +0000 UTC"},
        {"ID":"bbbb33334444","Image":"node:22","Names":"old-job","State":"exited","Size":"12MB","CreatedAt":"2026-10-01 10:00:00 +0000 UTC"}
      ],
      "Volumes": [{"Name":"pgdata","Size":"5GB","Links":"1"}],
      "BuildCache": [
        {"ID":"a","InUse":"false","Shared":"false","Size":"87.1MB","LastUsedAt":"2026-07-14 10:33:00.160166114 +0000 UTC"},
        {"ID":"b","InUse":"false","Shared":"false","Size":"20.7kB","LastUsedAt":"2026-07-15 10:33:00 +0000 UTC"},
        {"ID":"c","InUse":"true","Shared":"false","Size":"1GB","LastUsedAt":"2026-11-15 10:33:00 +0000 UTC"},
        {"ID":"d","InUse":"false","Shared":"true","Size":"2GB","LastUsedAt":"2026-11-15 10:33:00 +0000 UTC"}
      ]
    }"#;

    fn by_key(candidates: &[Candidate]) -> BTreeMap<&str, &Candidate> {
        candidates.iter().map(|c| (c.key.as_str(), c)).collect()
    }

    #[test]
    fn docker_offers_only_unused_images_stopped_containers_and_free_build_cache() {
        let candidates = parse_docker(DOCKER).unwrap();
        let found = by_key(&candidates);

        let keys: Vec<&str> = found.keys().copied().collect();
        assert_eq!(
            keys,
            [
                "build-cache",
                "container/bbbb33334444",
                "container/f912c22323b1",
                "image/sha256:cf09adf8c3eb",
                "image/sha256:dangling0000a",
                "image/sha256:twotags00000",
            ]
        );
        // Never: an image a container uses, a running container, an image that frees nothing, or a volume.
        assert!(!keys.iter().any(|k| k.contains("inuse") || k.contains("aaaa") || k.contains("shared") || k.contains("pgdata")));

        let rust = found["image/sha256:cf09adf8c3eb"];
        assert_eq!(rust.label, "rust:1.94-slim");
        assert_eq!(rust.size, 1_237_000_000);
        assert_eq!(rust.action, Action::RemoveImage(vec!["sha256:cf09adf8c3eb".into()]));

        let tagged_twice = found["image/sha256:twotags00000"];
        assert_eq!(tagged_twice.action, Action::RemoveImage(vec!["app:latest".into(), "app:v2".into()]));

        assert!(found["image/sha256:dangling0000a"].label.starts_with("Untagged image"));

        let cache = found["build-cache"];
        assert_eq!(cache.size, 87_100_000 + 20_700);
        assert_eq!(cache.action, Action::PruneBuildCache);
        assert_eq!(cache.modified, parse_time("2026-07-15 10:33:00 +0000 UTC").unwrap());

        assert_eq!(found["container/bbbb33334444"].action, Action::RemoveContainer("bbbb33334444".into()));
        assert_eq!(found["container/f912c22323b1"].label, "buildwipe-probe");
    }

    #[test]
    fn an_empty_docker_offers_nothing() {
        let empty = r#"{"Images":[],"Containers":[],"Volumes":[],"BuildCache":[]}"#;
        assert!(parse_docker(empty).unwrap().is_empty());
        assert!(parse_docker("not json").is_err());
    }

    #[test]
    fn podman_offers_only_unused_images_and_stopped_containers() {
        let images = r#"[
          {"Id":"1111aaaa2222bbbb","Names":["docker.io/library/alpine:latest"],"Size":7000000,"Containers":0,"Created":1700000000},
          {"Id":"3333cccc4444dddd","Names":["docker.io/library/nginx:latest"],"Size":190000000,"Containers":2,"Created":1700000000},
          {"Id":"5555eeee6666ffff","Names":null,"Size":3000,"Containers":0,"Created":1700000000}
        ]"#;
        let found = parse_podman_images(images).unwrap();
        let keys: Vec<&str> = found.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["image/1111aaaa2222bbbb", "image/5555eeee6666ffff"]);
        assert_eq!(found[0].label, "docker.io/library/alpine:latest");
        assert_eq!(found[0].modified, 1_700_000_000_000);
        assert_eq!(found[1].label, "Untagged image 5555eeee6666");

        let containers = r#"[
          {"Id":"c1","Names":["web"],"Image":"nginx","State":"running","Size":{"rootFsSize":1,"rwSize":5},"Created":1700000000,"IsInfra":false,"Pod":""},
          {"Id":"c2","Names":["job"],"Image":"alpine","State":"exited","Size":{"rootFsSize":1,"rwSize":4096},"Created":1700000000,"IsInfra":false,"Pod":""},
          {"Id":"c3","Names":["pod-infra"],"Image":"pause","State":"exited","Created":1700000000,"IsInfra":true,"Pod":"p1"},
          {"Id":"c4","Names":["in-pod"],"Image":"alpine","State":"exited","Created":1700000000,"IsInfra":false,"Pod":"p1"}
        ]"#;
        let found = parse_podman_containers(containers).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label, "job");
        assert_eq!(found[0].size, 4096);
        assert_eq!(found[0].action, Action::RemoveContainer("c2".into()));

        assert!(parse_podman_images("null").unwrap().is_empty());
        assert!(parse_podman_containers("").unwrap().is_empty());
    }

    #[test]
    fn sizes_parse_as_docker_prints_them() {
        assert_eq!(parse_size("1.237GB"), Some(1_237_000_000));
        assert_eq!(parse_size("20.7kB"), Some(20_700));
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("4.1kB (virtual 915MB)"), Some(4_100));
        assert_eq!(parse_size("2GiB"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("N/A"), None);
        assert_eq!(parse_size("12"), None);
    }

    #[test]
    fn times_parse_with_their_offset() {
        assert_eq!(parse_time("1970-01-01 00:00:00 +0000 UTC"), Some(0));
        assert_eq!(parse_time("2026-04-07 08:01:40 +0530 IST"), parse_time("2026-04-07 02:31:40 +0000 UTC"));
        assert_eq!(parse_time("2024-02-29 12:00:00.5 -0100"), Some(1_709_211_600_500));
        assert_eq!(parse_time("yesterday"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn refusals_are_explained() {
        let image = "Error response from daemon: conflict: unable to remove repository reference \"rust\" (must force) - container f912 is using its referenced image cf09";
        assert!(explain(Engine::Docker, image).contains("left alone"));
        let running = "Error response from daemon: cannot remove container \"/db\": container is running: stop the container before removing or force remove";
        assert!(explain(Engine::Docker, running).contains("running now"));
        let down = "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?";
        assert!(explain(Engine::Docker, down).contains("stopped running"));
        assert_eq!(explain(Engine::Podman, "something odd"), "something odd");
        assert!(is_already_gone("Error response from daemon: No such image: sha256:abc"));
        assert!(!is_already_gone("permission denied"));
    }

    #[test]
    fn a_missing_program_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_program("buildwipe-no-such-tool", &[dir.path().to_path_buf()]).is_none());

        let name = if cfg!(windows) { "fake-engine.exe" } else { "fake-engine" };
        std::fs::write(dir.path().join(name), b"").unwrap();
        assert_eq!(
            find_program("fake-engine", &[dir.path().to_path_buf()]),
            Some(dir.path().join(name))
        );
        // A relative folder could point anywhere, depending on where the app was started.
        assert!(find_program("fake-engine", &[PathBuf::from("relative")]).is_none());
    }
}
