//! The VPNs, VLESS through mihomo's API and OpenVPN through systemd: their state and toggles (switched by the
//! dotfiles' vless and openvpn-ctl).

mod service;
mod widget;

use super::{widget, Fut, Module, TOGGLE};
use crate::cc::Show;
use crate::services::Ctx;

pub const VLESS: Module = Module {
    id: "vless",
    state: Some(vless),
    widgets: &[widget("vless", "VLESS", "network-vpn-symbolic", TOGGLE, widget::vless).bar(Show::Active)],
    ..Module::NONE
};

pub const OPENVPN: Module = Module {
    id: "openvpn",
    state: Some(openvpn),
    widgets: &[widget("openvpn", "OpenVPN", "network-vpn-symbolic", TOGGLE, widget::openvpn).bar(Show::Active)],
    ..Module::NONE
};

fn vless(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::vless_state())
}

fn openvpn(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::openvpn_state(c))
}
