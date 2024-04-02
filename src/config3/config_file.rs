use std::{
    fmt::Display,
    mem,
    sync::atomic::{AtomicBool, Ordering},
};

use color_eyre::Result;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{sync::watch, task::JoinHandle};

use super::file_handler::FileHander;

type ConfigError = String;
#[derive(Debug, Clone)]
pub enum ConfigResult<T> {
    Valid(T),
    Invalid(T, ConfigError),
}

impl<T: Default> ConfigResult<T> {
    fn with_error(&mut self, errors: &impl Display) {
        let v = match self {
            ConfigResult::Valid(v) => v,
            ConfigResult::Invalid(v, _) => v,
        };
        let v = mem::take(v);

        *self = Self::Invalid(v, errors.to_string());
    }

    fn get(&self) -> &T {
        match self {
            ConfigResult::Valid(ref v) => v,
            ConfigResult::Invalid(ref v, _) => v,
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        match self {
            ConfigResult::Valid(v) => v,
            ConfigResult::Invalid(v, _) => v,
        }
    }
}

impl<T: Default> Default for ConfigResult<T> {
    fn default() -> Self {
        Self::Valid(T::default())
    }
}

impl<T> From<T> for ConfigResult<T> {
    fn from(value: T) -> Self {
        ConfigResult::Valid(value)
    }
}

pub struct ConfigurationFile<T> {
    serializer_thread: JoinHandle<()>,
    deserializer_thread: JoinHandle<()>,
    rx: watch::Receiver<ConfigResult<T>>,
    tx: watch::Sender<ConfigResult<T>>,
}

pub(super) static HAS_DESERIALIZED: AtomicBool = AtomicBool::new(true);

impl<T: Default + Serialize + DeserializeOwned + Send + Sync + 'static> ConfigurationFile<T> {
    pub fn new(file: &FileHander) -> Result<Self> {
        let (raw_tx, mut raw_rx) = (file.tx(), file.rx());

        let initial_value = match serde_json::from_str::<T>(&std::fs::read_to_string(file.path())?)
        {
            Ok(v) => ConfigResult::Valid(v),
            Err(err) => ConfigResult::Invalid(T::default(), err.to_string()),
        };

        let (config_tx, mut config_rx) = watch::channel(initial_value);

        config_rx.mark_changed();

        let config_tx2 = config_tx.clone();
        let deserializer_thread = tokio::spawn(async move {
            let config_tx = config_tx2;
            while let Ok(()) = raw_rx.changed().await {
                let contents = raw_rx.borrow();
                let parsed = serde_json::from_str::<T>(&contents);
                HAS_DESERIALIZED.store(true, Ordering::SeqCst);

                let config = match parsed {
                    Err(err) => {
                        println!("Error while parsing config: {err}");

                        config_tx.send_modify(|current| {
                            println!("Send from deserializer (error)!");
                            current.with_error(&err);
                        });

                        continue;
                    }
                    Ok(config) => config,
                };

                config_tx.send_replace(config.into());
            }
        });

        let config_rx2 = config_rx.clone();
        let serializer_thread = tokio::spawn(async move {
            let mut config_rx = config_rx2;
            while let Ok(()) = config_rx.changed().await {
                if HAS_DESERIALIZED.load(Ordering::SeqCst) {
                    HAS_DESERIALIZED.store(false, Ordering::SeqCst);
                    continue;
                }

                let config = serde_json::to_string(config_rx.borrow().get())
                    .expect("Internal config object should always be valid.");
                raw_tx.send_if_modified(|current| {
                    let changed = current != &config;
                    if changed {
                        *current = config;
                    }

                    changed
                });
            }
        });

        Ok(Self {
            serializer_thread,
            deserializer_thread,
            rx: config_rx,
            tx: config_tx,
        })
    }

    pub fn rx(&self) -> watch::Receiver<ConfigResult<T>> {
        self.rx.clone()
    }

    pub fn tx(&self) -> watch::Sender<ConfigResult<T>> {
        self.tx.clone()
    }

    pub fn stop(self) {
        self.deserializer_thread.abort();
        self.serializer_thread.abort();
    }
}
