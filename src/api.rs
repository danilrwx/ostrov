//! ostrov's face on the session bus, dev.ostrov.Shell at /dev/ostrov/Shell (docs/plugins.md): Run(as) -> s,
//! `ostrov ARGS`'s own commands and outcome; State() -> s, the services' state as JSON (ostrov dump); the
//! StateChanged(s) signal as it changes; OpenPanel(s), the control centre with that widget's menu unfolded ("" for
//! none); Toast(s, s), a notification of ostrov's own. The server runs in a Tokio thread of its own, the commands
//! on GTK's, through main.rs's one dispatch. Nothing if the name is taken (another ostrov has it).

use std::rc::Rc;

use gtk4::glib;
use zbus::object_server::SignalEmitter;

use crate::hub::Hub;

const NAME: &str = "dev.ostrov.Shell";
const PATH: &str = "/dev/ostrov/Shell";

/// A command for GTK's thread, and where its outcome goes.
type Call = (Vec<String>, async_channel::Sender<Result<String, String>>);

struct Shell {
    calls: async_channel::Sender<Call>,
}

impl Shell {
    async fn call(&self, args: Vec<String>) -> zbus::fdo::Result<String> {
        let (tx, rx) = async_channel::bounded(1);
        fn gone<E>(_: E) -> zbus::fdo::Error {
            zbus::fdo::Error::Failed("ostrov is going".into())
        }
        self.calls.send((args, tx)).await.map_err(gone)?;
        rx.recv().await.map_err(gone)?.map_err(zbus::fdo::Error::Failed)
    }
}

#[zbus::interface(name = "dev.ostrov.Shell")]
impl Shell {
    async fn run(&self, args: Vec<String>) -> zbus::fdo::Result<String> {
        self.call(args).await
    }

    async fn state(&self) -> zbus::fdo::Result<String> {
        self.call(vec!["dump".into()]).await
    }

    async fn open_panel(&self, menu: String) -> zbus::fdo::Result<()> {
        self.call(vec!["menu".into(), menu]).await.map(drop)
    }

    async fn toast(&self, title: String, body: String) -> zbus::fdo::Result<()> {
        self.call(vec!["toast".into(), title, body]).await.map(drop)
    }

    #[zbus(signal)]
    async fn state_changed(e: &SignalEmitter<'_>, state: String) -> zbus::Result<()>;
}

/// The server started, the hub's every state signalled.
pub fn start(hub: &Rc<Hub>) {
    let (calls, asked) = async_channel::unbounded::<Call>();
    let (states, changed) = async_channel::unbounded::<String>();
    hub.on(move |st| {
        if !st.is_null() {
            let _ = states.send_blocking(st.to_string());
        }
    });
    std::thread::spawn(move || serve(calls, changed));
    glib::spawn_future_local(async move {
        while let Ok((args, reply)) = asked.recv().await {
            let r = crate::command(&args, None);
            glib::spawn_future_local(async move {
                let _ = reply.send(r.await).await;
            });
        }
    });
}

fn serve(calls: async_channel::Sender<Call>, changed: async_channel::Receiver<String>) {
    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return eprintln!("ostrov: {NAME}: {e}"),
    };
    rt.block_on(async move {
        let built =
            async { zbus::connection::Builder::session()?.name(NAME)?.serve_at(PATH, Shell { calls })?.build().await };
        let conn = match built.await {
            Ok(c) => c,
            Err(e) => return eprintln!("ostrov: {NAME}: {e}"),
        };
        let Ok(iface) = conn.object_server().interface::<_, Shell>(PATH).await else { return };
        while let Ok(st) = changed.recv().await {
            let _ = Shell::state_changed(iface.signal_emitter(), st).await;
        }
    });
}
