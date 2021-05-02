use std::{error::Error, time::Instant};

use bevy::prelude::*;
use bevy_tts::Tts;
use derive_more::{Deref, DerefMut};

use crate::error::error_handler;

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Log(pub Vec<LogEntry>);

impl Log {
    pub fn push<S: Into<String>>(&mut self, message: S) {
        self.0.push(LogEntry {
            time: Instant::now(),
            message: message.into(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub time: Instant,
    pub message: String,
}

fn setup(mut commands: Commands) {
    commands.spawn().insert(Log::default());
}

fn read_log(
    mut tts: ResMut<Tts>,
    mut position: Local<usize>,
    log: Query<&Log, Changed<Log>>,
) -> Result<(), Box<dyn Error>> {
    for log in log.iter() {
        for (index, entry) in log.iter().enumerate() {
            if index >= *position {
                tts.speak(entry.message.clone(), false)?;
                *position = index + 1;
            }
        }
    }
    Ok(())
}

pub struct LogPlugin;

impl Plugin for LogPlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_startup_system(setup.system()).add_system_to_stage(
            CoreStage::PostUpdate,
            read_log
                .system()
                .chain(error_handler.system())
                .after(crate::visibility::LOG_VISIBLE_LABEL),
        );
    }
}
