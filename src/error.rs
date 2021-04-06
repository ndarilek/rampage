use std::error::Error;
use std::{panic, thread};

use backtrace::Backtrace;
use bevy::prelude::*;

pub fn error_handler(In(result): In<Result<(), Box<dyn Error>>>) {
    if let Err(e) = result {
        error!("{}", e);
    }
}

fn init_panic_handler() {
    panic::set_hook(Box::new(|info| {
        let backtrace = Backtrace::default();

        let thread = thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &**s,
                None => "Box<Any>",
            },
        };

        match info.location() {
            Some(location) => {
                error!(
                    target: "panic", "thread '{}' panicked at '{}': {}:{}{:?}",
                    thread,
                    msg,
                    location.file(),
                    location.line(),
                    backtrace
                );
            }
            None => error!(
                target: "panic",
                "thread '{}' panicked at '{}'{:?}",
                thread,
                msg,
                backtrace
            ),
        }
    }));
}

pub struct ErrorPlugin;

impl Plugin for ErrorPlugin {
    fn build(&self, _: &mut AppBuilder) {
        init_panic_handler();
    }
}
