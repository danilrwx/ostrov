//! What the services share of D-Bus: a method called by its interface's name, a property set or read, an
//! ObjectManager's objects, a service's signals as kicks, and an error as the user is told it.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use zbus::zvariant::{self, DynamicDeserialize, DynamicType, OwnedObjectPath, OwnedValue};

use super::{Ctx, Kick, Res};

/// An interface's properties, and every object's interfaces, as GetManagedObjects hands them.
pub type Props = HashMap<String, OwnedValue>;
pub type Objects = HashMap<OwnedObjectPath, HashMap<String, Props>>;

/// A D-Bus error as the user is told it: its message alone, or its name when it has none, as godbus prints it.
pub fn err(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, desc, _) => desc.unwrap_or_else(|| name.to_string()),
        e => e.to_string(),
    }
}

/// Calls a method, named with its interface (net.connman.iwd.Station.Scan), on a service's object, its reply's
/// body read as R.
pub async fn call<B, R>(
    conn: &zbus::Connection,
    service: &str,
    path: &str,
    method: &str,
    body: &B,
) -> Result<R, String>
where
    B: Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    let (iface, name) = method.rsplit_once('.').ok_or_else(|| format!("no interface in {method}"))?;
    let reply = conn.call_method(Some(service), path, Some(iface), name, body).await.map_err(err)?;
    reply.body().deserialize().map_err(err)
}

/// Sets the boolean property k, named with its interface (org.bluez.Device1.Trusted), of a service's object.
pub async fn set_property(conn: &zbus::Connection, service: &str, path: &str, k: &str, v: bool) -> Res {
    let (iface, name) = k.rsplit_once('.').ok_or_else(|| format!("no interface in {k}"))?;
    call(conn, service, path, "org.freedesktop.DBus.Properties.Set", &(iface, name, zvariant::Value::from(v))).await
}

/// The property k of an interface's properties, if it is there and of the type; a missing one (a device a scan
/// found has no Icon yet) is None.
pub fn prop<'a, T>(props: Option<&'a Props>, k: &str) -> Option<T>
where
    T: TryFrom<&'a zvariant::Value<'a>>,
    <T as TryFrom<&'a zvariant::Value<'a>>>::Error: Into<zvariant::Error>,
{
    let v: &'a zvariant::Value<'a> = props?.get(k)?;
    v.downcast_ref().ok()
}

pub async fn managed(conn: &zbus::Connection, service: &str) -> Result<Objects, String> {
    call(conn, service, "/", "org.freedesktop.DBus.ObjectManager.GetManagedObjects", &()).await
}

/// Every signal of a service's on the system bus (iwd's, BlueZ's...), as a kick.
pub async fn signals(c: Arc<Ctx>, sender: &str, kick: Kick) {
    use futures_util::StreamExt;
    let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(sender).map(|b| b.build()) else {
        return;
    };
    let Ok(mut s) = zbus::MessageStream::for_match_rule(rule, &c.system, None).await else { return };
    while s.next().await.is_some() {
        let _ = kick.send(()).await;
    }
}
