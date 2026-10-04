//! A game focused played in a power profile of its own ([games]).

pub mod service;

use super::{Fut, Module};
use crate::services::{Ctx, Kick};

pub const MODULE: Module = Module { id: "games", worker: Some(worker), ..Module::NONE };

fn worker(c: std::sync::Arc<Ctx>, _: Kick) -> Fut<'static, ()> {
    Box::pin(service::run(c))
}
