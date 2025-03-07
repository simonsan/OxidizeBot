use crate::sys::Notification;
use failure::Error;
use futures::{channel::oneshot, future};
use std::path::Path;

#[derive(Clone)]
pub struct System;

impl System {
    pub async fn wait_for_shutdown(&self) -> Result<(), oneshot::Canceled> {
        future::pending().await
    }

    pub async fn wait_for_restart(&self) -> Result<(), oneshot::Canceled> {
        future::pending().await
    }

    pub fn clear(&self) {}

    pub fn error(&self, _error: String) {}

    pub fn notification(&self, _: Notification) {}

    pub fn is_running(&self) -> bool {
        true
    }

    pub fn join(&self) -> Result<(), Error> {
        Ok(())
    }

    pub fn is_installed(&self) -> Result<bool, Error> {
        Ok(true)
    }

    pub fn install(&self) -> Result<(), Error> {
        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), Error> {
        Ok(())
    }
}

pub fn setup(_root: &Path, _log_file: &Path) -> Result<System, Error> {
    Ok(System)
}
