// SPDX-License-Identifier: GPL-3.0-only

//! Apps' volumes, the microphones, and who is listening or watching: through
//! PipeWire's PulseAudio side, and /proc for cameras opened directly.

use std::{collections::HashSet, time::Duration};

use cosmic::iced::{Subscription, stream};
use futures::{SinkExt, channel::mpsc};
use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use crate::{
    latest::Latest,
    services::{Volume, pactl},
};

/// The icon of an app that doesn't name one, as Firefox: by its program
/// ("firefox-bin" is firefox) or its name, if the icon theme has it.
fn guess_icon(app: &str, binary: Option<&str>) -> Option<String> {
    let binary = binary.map(|b| b.trim_end_matches("-bin").to_lowercase());
    let name = app.to_lowercase().replace(' ', "-");
    binary
        .into_iter()
        .chain(std::iter::once(name))
        .find(|candidate| cosmic::widget::icon::from_name(candidate.as_str()).path().is_some())
}

/// An app playing sound, all its streams together.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub indices: Vec<u32>,
    pub app: String,
    pub icon: Option<String>,
    pub level: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mixer {
    pub streams: Vec<Stream>,
    pub sources: Vec<Source>,
    pub default_source: String,
    pub input: Option<Volume>,
}

async fn list(kind: &str) -> Vec<Value> {
    let Ok(output) = pactl().args(["-f", "json", "list", kind]).output().await else {
        return Vec::new();
    };
    serde_json::from_slice(&output.stdout).unwrap_or_default()
}

/// The channels' mean, 1.0 being 100%.
fn level(volume: &Value) -> f32 {
    let levels: Vec<f32> = volume
        .as_object()
        .into_iter()
        .flat_map(|channels| channels.values())
        .filter_map(|channel| channel["value"].as_f64())
        .map(|value| value as f32 / 65536.0)
        .collect();
    if levels.is_empty() {
        0.0
    } else {
        levels.iter().sum::<f32>() / levels.len() as f32
    }
}

fn property<'a>(stream: &'a Value, key: &str) -> Option<&'a str> {
    stream["properties"][key].as_str().filter(|v| !v.is_empty())
}

/// What a sound card's input is, by its port: "Digital Microphone"; a
/// device by its name.
fn short_description(device: &Value) -> Option<&str> {
    let name = device["name"].as_str().unwrap_or_default();
    let active = device["active_port"].as_str();
    device["ports"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|port| port["name"].as_str() == active)
        .filter(|_| !name.starts_with("bluez"))
        .and_then(|port| port["description"].as_str())
        .or(device["description"].as_str())
}

fn is_monitor(source: &Value) -> bool {
    source["name"].as_str().is_some_and(|n| n.ends_with(".monitor"))
        || source["monitor_source"].as_str().is_some_and(|m| !m.is_empty())
}

pub async fn mixer() -> Mixer {
    let mut streams: Vec<Stream> = Vec::new();
    for input in list("sink-inputs").await {
        let Some(index) = input["index"].as_u64() else {
            continue;
        };
        let Some(app) = property(&input, "application.name").or(property(&input, "node.name"))
        else {
            continue;
        };
        // Speech Dispatcher keeps a silent stream open: not an app.
        if app.starts_with("speech-dispatcher")
            || property(&input, "application.process.binary") == Some("sd_dummy")
        {
            continue;
        }
        let muted = input["mute"].as_bool().unwrap_or(false);
        let level = level(&input["volume"]);
        // Tabs of one browser, say: one app.
        if let Some(stream) = streams.iter_mut().find(|s| s.app == app) {
            stream.indices.push(index as u32);
            stream.level = stream.level.max(level);
            stream.muted &= muted;
            continue;
        }
        streams.push(Stream {
            indices: vec![index as u32],
            app: app.to_owned(),
            icon: property(&input, "application.icon_name")
                .map(str::to_owned)
                .or_else(|| guess_icon(app, property(&input, "application.process.binary"))),
            level,
            muted,
        });
    }

    let default_source = pactl()
        .arg("get-default-source")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let mut sources = Vec::new();
    let mut input = None;
    for source in list("sources").await {
        if is_monitor(&source) {
            continue;
        }
        let (Some(name), Some(description)) = (source["name"].as_str(), short_description(&source))
        else {
            continue;
        };
        if name == default_source {
            input = Some(Volume {
                level: level(&source["volume"]),
                muted: source["mute"].as_bool().unwrap_or(false),
            });
        }
        sources.push(Source {
            name: name.to_owned(),
            description: description.to_owned(),
        });
    }
    Mixer {
        streams,
        sources,
        default_source,
        input,
    }
}

fn percent(level: f32) -> String {
    format!("{:.0}%", level.clamp(0.0, 1.5) * 100.0)
}

pub async fn set_stream_volume(indices: Vec<u32>, level: f32) {
    // By app: one moved right after another still gets its last value.
    static LATEST: Latest<Vec<u32>, f32> = Latest::new();
    LATEST
        .send(indices, level, |indices, level| async move {
            for index in indices {
                _ = pactl()
                    .args(["set-sink-input-volume", &index.to_string(), &percent(level)])
                    .status()
                    .await;
            }
        })
        .await;
}

pub async fn set_stream_mute(indices: Vec<u32>, muted: bool) {
    let muted = if muted { "1" } else { "0" };
    for index in indices {
        _ = pactl()
            .args(["set-sink-input-mute", &index.to_string(), muted])
            .status()
            .await;
    }
}

pub async fn set_source(name: String) {
    _ = pactl().args(["set-default-source", &name]).status().await;
}

pub async fn set_input_volume(level: f32) {
    static LATEST: Latest<(), f32> = Latest::new();
    LATEST
        .send((), level, |(), level| async move {
            _ = pactl()
                .args(["set-source-volume", "@DEFAULT_SOURCE@", &percent(level)])
                .status()
                .await;
        })
        .await;
}

/// The apps using the microphone and the camera now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Privacy {
    pub microphone: Vec<String>,
    pub camera: Vec<String>,
}

impl Privacy {
    pub fn is_empty(&self) -> bool {
        self.microphone.is_empty() && self.camera.is_empty()
    }
}

/// Apps recording from a microphone: not from a monitor of an output, which
/// is sound capture, nor level meters.
async fn recording() -> Vec<String> {
    let monitors: HashSet<u64> = list("sources")
        .await
        .iter()
        .filter(|source| is_monitor(source))
        .filter_map(|source| source["index"].as_u64())
        .collect();
    let mut apps: Vec<String> = Vec::new();
    for output in list("source-outputs").await {
        let from_monitor = output["source"].as_u64().is_some_and(|s| monitors.contains(&s));
        let meter = property(&output, "stream.monitor") == Some("true")
            || property(&output, "media.name") == Some("Peak detect");
        if from_monitor || meter || output["corked"].as_bool().unwrap_or(false) {
            continue;
        }
        if let Some(app) = property(&output, "application.name") {
            if !apps.iter().any(|a| a == app) {
                apps.push(app.to_owned());
            }
        }
    }
    apps
}

/// Processes with a camera open, by name.
fn camera_holders() -> Vec<(u32, String)> {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut holders = Vec::new();
    for process in processes.flatten() {
        let Some(pid) = process.file_name().to_str().and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        let holds_camera = fds.flatten().any(|fd| {
            std::fs::read_link(fd.path())
                .is_ok_and(|target| target.to_string_lossy().starts_with("/dev/video"))
        });
        if holds_camera {
            let name = std::fs::read_to_string(process.path().join("comm")).unwrap_or_default();
            holders.push((pid, name.trim().to_owned()));
        }
    }
    holders
}

/// Apps getting a camera's picture through PipeWire, as the camera portal
/// gives it: streams linked to a camera device's node. Screen sharing comes
/// from no device.
async fn pipewire_camera_users() -> Vec<String> {
    let Ok(output) = crate::host::command("pw-dump", &[]).output().await else {
        return Vec::new();
    };
    let objects: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    let props = |object: &Value| object["info"]["props"].clone();
    let cameras: HashSet<u64> = objects
        .iter()
        .filter(|o| {
            let props = props(o);
            props["media.class"] == "Video/Source" && props["device.api"].is_string()
        })
        .filter_map(|o| o["id"].as_u64())
        .collect();
    let readers: HashSet<u64> = objects
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Link")
        .filter(|o| {
            o["info"]["output-node-id"]
                .as_u64()
                .is_some_and(|id| cameras.contains(&id))
        })
        .filter_map(|o| o["info"]["input-node-id"].as_u64())
        .collect();
    let mut apps: Vec<String> = Vec::new();
    for object in objects.iter().filter(|o| o["id"].as_u64().is_some_and(|id| readers.contains(&id))) {
        let props = props(object);
        let name = props["application.name"]
            .as_str()
            .or(props["node.name"].as_str())
            .unwrap_or_default();
        if !name.is_empty() && !apps.iter().any(|a| a == name) {
            apps.push(name.to_owned());
        }
    }
    apps
}

/// The same, from a sandbox, which sees neither the host's processes nor
/// its cameras: asked of the host in one go.
async fn host_camera_holders() -> Vec<(u32, String)> {
    const SCRIPT: &str = r#"for fd in $(find /proc/[0-9]*/fd -maxdepth 1 -lname '/dev/video*' 2>/dev/null); do
        pid=${fd#/proc/}; pid=${pid%%/*}
        printf '%s %s\n' "$pid" "$(cat /proc/$pid/comm 2>/dev/null)"
    done"#;
    let Ok(output) = crate::host::command("sh", &[]).args(["-c", SCRIPT]).output().await else {
        return Vec::new();
    };
    let mut holders: Vec<(u32, String)> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, name) = line.split_once(' ')?;
            Some((pid.parse().ok()?, name.to_owned()))
        })
        .collect();
    holders.dedup();
    holders
}

async fn camera() -> Vec<String> {
    let holders = if crate::host::in_flatpak() {
        host_camera_holders().await
    } else {
        tokio::task::spawn_blocking(camera_holders)
            .await
            .unwrap_or_default()
    };
    let mut apps = Vec::new();
    let mut through_pipewire = false;
    for (_, name) in holders {
        if name == "pipewire" || name == "wireplumber" {
            through_pipewire = true;
        } else if !apps.contains(&name) {
            apps.push(name);
        }
    }
    if through_pipewire {
        for app in pipewire_camera_users().await {
            if !apps.contains(&app) {
                apps.push(app);
            }
        }
    }
    apps
}

/// How many times the cameras are open now, counting openings and closings
/// through inotify: nothing to do while none is. Apps listing cameras open
/// and close them at once, which counts for nothing. Gives up quietly
/// without inotify or cameras.
fn watch_cameras(tx: tokio::sync::mpsc::UnboundedSender<i32>) {
    let cameras: Vec<std::ffi::CString> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("video"))
        .filter_map(|entry| {
            std::ffi::CString::new(entry.path().as_os_str().as_encoded_bytes()).ok()
        })
        .collect();
    if cameras.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        // SAFETY: plain syscalls on a descriptor this thread owns, and
        // reading the events the kernel wrote into the buffer.
        unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC);
            if fd < 0 {
                return;
            }
            for camera in &cameras {
                libc::inotify_add_watch(fd, camera.as_ptr(), libc::IN_OPEN | libc::IN_CLOSE);
            }
            // Openings, minus closings, of each camera.
            let mut open: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
            let mut last = 0;
            let mut buffer = [0u8; 4096];
            loop {
                let read = libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len());
                if read <= 0 {
                    libc::close(fd);
                    return;
                }
                let mut offset = 0;
                while offset + std::mem::size_of::<libc::inotify_event>() <= read as usize {
                    let event = std::ptr::read_unaligned(
                        buffer.as_ptr().add(offset).cast::<libc::inotify_event>(),
                    );
                    let count = open.entry(event.wd).or_default();
                    if event.mask & libc::IN_OPEN != 0 {
                        *count += 1;
                    } else if event.mask & libc::IN_CLOSE != 0 {
                        *count = (*count - 1).max(0);
                    }
                    offset += std::mem::size_of::<libc::inotify_event>() + event.len as usize;
                }
                let total: i32 = open.values().sum();
                if total != last {
                    last = total;
                    if tx.send(total).is_err() {
                        libc::close(fd);
                        return;
                    }
                }
            }
        }
    });
}

pub fn privacy_subscription() -> Subscription<Privacy> {
    Subscription::run_with("privacy", |_| {
        stream::channel(4, |mut output: mpsc::Sender<Privacy>| async move {
            let mut last = None;
            let mut events = crate::services::pactl_subscribe()
                .stdout(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .ok()
                .and_then(|mut child| {
                    let stdout = child.stdout.take()?;
                    Some((child, tokio::io::BufReader::new(stdout).lines()))
                });
            // Cameras are looked at when one is opened or closed, and now and
            // then for the rest; recording, as streams change.
            let (tx, mut opened) = tokio::sync::mpsc::unbounded_channel();
            watch_cameras(tx);
            // Whether a camera may be open: at first, not known.
            let mut cameras_open = true;
            // A sandbox can't watch the cameras: it looks more often.
            let sandboxed = crate::host::in_flatpak();
            let mut timer = tokio::time::interval(Duration::from_secs(if sandboxed { 5 } else { 30 }));
            let mut privacy = Privacy {
                microphone: recording().await,
                camera: Vec::new(),
            };
            loop {
                match &mut events {
                    Some((_, lines)) => tokio::select! {
                        line = lines.next_line() => match line {
                            Ok(Some(line)) if line.contains("source-output") => {
                                privacy.microphone = recording().await;
                            }
                            Ok(Some(_)) => continue,
                            _ => events = None,
                        },
                        _ = timer.tick() => {
                            if cameras_open || sandboxed {
                                privacy.camera = camera().await;
                            }
                        }
                        Some(mut count) = opened.recv() => {
                            // Settled: one opening comes with a few events.
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            while let Ok(newer) = opened.try_recv() {
                                count = newer;
                            }
                            cameras_open = count > 0;
                            privacy.camera = if cameras_open { camera().await } else { Vec::new() };
                        }
                    },
                    None => {
                        timer.tick().await;
                        privacy.camera = camera().await;
                        privacy.microphone = recording().await;
                    }
                }
                if last.as_ref() != Some(&privacy) {
                    last = Some(privacy.clone());
                    if output.send(privacy.clone()).await.is_err() {
                        return;
                    }
                }
            }
        })
    })
}
