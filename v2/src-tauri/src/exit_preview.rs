//! Development-only simulation and separate preferences. Never drives real quit or ADB.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::exit_lifecycle::{Attempt, Controller, ExitPolicy, Phase, Preference, Request};

#[derive(Clone, Default)]
pub struct ExitPreviewState(Arc<Mutex<Option<Session>>>);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Read,
    Save {
        policy: ExitPolicy,
    },
    RequestQuit,
    Confirm {
        policy: ExitPolicy,
        remember: bool,
        prompt_generation: u64,
    },
    Cancel {
        prompt_generation: u64,
    },
    RestartSimulation,
    UpdaterRestart,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub preference: Preference,
    pub phase: &'static str,
    pub prompt_generation: u64,
    pub prompt_open: bool,
    pub admission_open: bool,
    pub stop_available: bool,
    pub message: String,
    pub storage_warning: Option<String>,
}

struct Session {
    path: PathBuf,
    preference: Preference,
    controller: Controller,
    pending: Option<Attempt>,
    generation: u64,
    message: String,
    storage_warning: Option<String>,
}

impl ExitPreviewState {
    pub fn configured(data_dir: &Path) -> Self {
        if cfg!(debug_assertions) && std::env::var("SHIELD_ADB_EXIT_PREVIEW").as_deref() == Ok("1")
        {
            Self::development(data_dir)
        } else {
            Self::default()
        }
    }

    pub fn development(data_dir: &Path) -> Self {
        if !cfg!(debug_assertions) {
            return Self::default();
        }
        let path = data_dir
            .join("development-previews")
            .join("adb-exit-v1.json");
        Self(Arc::new(Mutex::new(Some(Session::load(path)))))
    }

    pub fn apply(&self, action: Action) -> Result<Snapshot, String> {
        let mut state = self.0.lock().map_err(|_| "Preview state unavailable.")?;
        state
            .as_mut()
            .ok_or("Development preview is disabled.")?
            .apply(action)
    }
}

#[tauri::command]
pub async fn exit_settings_preview(
    preview: State<'_, ExitPreviewState>,
    action: Action,
) -> Result<Snapshot, String> {
    let preview = preview.inner().clone();
    tauri::async_runtime::spawn_blocking(move || preview.apply(action))
        .await
        .map_err(|_| "Preview request failed.".to_owned())?
}

fn read_preference(path: &Path) -> (Preference, Option<String>) {
    let read = || -> Result<Preference, String> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Preference::default())
            }
            Err(_) => {
                return Err("Could not read preview preferences; using safe defaults.".to_owned())
            }
        };
        let mut text = String::new();
        file.take(4097)
            .read_to_string(&mut text)
            .map_err(|_| "Unreadable preview preference; using safe defaults.")?;
        let decoded = Preference::decode(&text);
        if text.len() > 4096 || serde_json::from_str::<Preference>(&text).ok() != Some(decoded) {
            return Err("Invalid preview preference; using safe defaults.".to_owned());
        }
        if decoded.policy == ExitPolicy::AlwaysStop {
            return Err(
                "Always stop is unavailable; using Leave running without remembering a choice."
                    .to_owned(),
            );
        }
        Ok(decoded)
    };
    match read() {
        Ok(value) => (value, None),
        Err(warning) => (Preference::default(), Some(warning)),
    }
}

impl Session {
    fn load(path: PathBuf) -> Self {
        let (preference, storage_warning) = read_preference(&path);
        Self {
            path,
            preference,
            controller: Controller::new(preference),
            pending: None,
            generation: 0,
            message: "Simulation only: the app and shared ADB server stay running.".to_owned(),
            storage_warning,
        }
    }

    fn persist(&self, preference: Preference) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Preview storage unavailable.")?;
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(preference.encode()?.as_bytes())?;
            file.as_file().sync_all()?;
            file.persist(&self.path)?;
            Ok(())
        };
        write().map_err(|_| "Could not save preview preference; no choice was applied.".to_owned())
    }

    fn supported(policy: ExitPolicy) -> Result<(), String> {
        if policy == ExitPolicy::AlwaysStop {
            Err("Always stop is unavailable: the shutdown executor is not enabled. No preference was saved.".to_owned())
        } else {
            Ok(())
        }
    }

    fn finish_simulation(&mut self, attempt: Attempt) -> Result<(), String> {
        self.controller
            .finish_drain(attempt)
            .map_err(|_| "Preview drain failed.")?;
        self.controller
            .finish_local_teardown(attempt)
            .map_err(|_| "Preview local teardown failed.")?;
        self.pending = None;
        self.message =
            "Simulated exit completed. Shared ADB left running; the app remains open.".to_owned();
        Ok(())
    }

    fn apply(&mut self, action: Action) -> Result<Snapshot, String> {
        match action {
            Action::Read => {}
            Action::Save { policy } => {
                Self::supported(policy)?;
                if self.controller.phase() != Phase::Running {
                    return Err("Restart the simulation or finish its prompt first.".to_owned());
                }
                let preference = Preference::remembered(policy);
                self.persist(preference)?;
                self.preference = preference;
                self.controller = Controller::new(preference);
                self.storage_warning = None;
                self.message =
                    "Preview preference saved. Real quit behavior is unchanged.".to_owned();
            }
            Action::RequestQuit => match self.controller.request_quit(false) {
                Request::Prompt(attempt) => {
                    self.pending = Some(attempt);
                    self.generation = self
                        .generation
                        .checked_add(1)
                        .ok_or("Prompt generation exhausted.")?;
                    self.message =
                        "Choose how the simulated quit should handle shared ADB.".to_owned();
                }
                Request::Draining(attempt) => self.finish_simulation(attempt)?,
                Request::AlreadyPending => {
                    self.message =
                        "A simulated exit is already pending; repeated requests coalesce."
                            .to_owned()
                }
                _ => return Err("Unexpected preview request.".to_owned()),
            },
            Action::Cancel { prompt_generation } => {
                self.require_prompt(prompt_generation)?;
                let attempt = self.pending.ok_or("No preview prompt is open.")?;
                self.controller
                    .cancel_prompt(attempt)
                    .map_err(|_| "This prompt is no longer current.")?;
                self.pending = None;
                self.message = "Canceled. No preference or admission changes.".to_owned();
            }
            Action::Confirm {
                policy,
                remember,
                prompt_generation,
            } => {
                self.require_prompt(prompt_generation)?;
                Self::supported(policy)?;
                let attempt = self.pending.ok_or("No preview prompt is open.")?;
                if self.controller.phase() != Phase::Prompting {
                    return Err("This prompt is no longer current.".to_owned());
                }
                if remember {
                    let preference = Preference::remembered(policy);
                    self.persist(preference)?;
                    self.storage_warning = None;
                }
                self.controller
                    .confirm(attempt, policy, remember)
                    .map_err(|_| "This prompt is no longer current.")?;
                self.preference = self.controller.preference();
                self.finish_simulation(attempt)?;
            }
            Action::RestartSimulation => {
                let generation = self.generation;
                *self = Self::load(self.path.clone());
                self.generation = generation;
            }
            Action::UpdaterRestart => {
                let _request = self.controller.request_quit(true);
                self.pending = None;
                self.message = "Updater restart simulated: no app restart or shared-server stop was performed.".to_owned();
            }
        }
        Ok(self.snapshot())
    }

    fn require_prompt(&self, generation: u64) -> Result<(), String> {
        if self.pending.is_none()
            || self.controller.phase() != Phase::Prompting
            || generation != self.generation
        {
            return Err("This prompt is no longer current.".to_owned());
        }
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        let phase = self.controller.phase();
        Snapshot {
            preference: self.preference,
            phase: match phase {
                Phase::Running => "running",
                Phase::Prompting => "prompting",
                Phase::Restarting => "restarting",
                Phase::Finished => "finished",
                _ => "blocked",
            },
            prompt_generation: self.generation,
            prompt_open: phase == Phase::Prompting,
            admission_open: matches!(phase, Phase::Running | Phase::Prompting),
            stop_available: false,
            message: self.message.clone(),
            storage_warning: self.storage_warning.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
