//! Where the computer is, for the night light's sunset and the weather: `ostrov location CITY|LAT LON`.

use super::{words, Fut, Module};
use crate::services::{location, Ctx, Res};

pub const MODULE: Module = Module { id: "location", forms: &["CITY...", "LAT LON"], run: Some(run), ..Module::NONE };

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { location::cmd(&words(&args)).await })
}
