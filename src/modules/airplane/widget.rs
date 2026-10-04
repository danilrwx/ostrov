use crate::cc::{Ctx, Widget};
use crate::hub::service;
use crate::i18n::t;
use crate::ui::Toggle;

/// Airplane Mode, a toggle.
pub fn airplane(_: &Ctx) -> Widget {
    let tg = Toggle::new("airplane-mode-symbolic", t("Airplane Mode"), || service(&["airplane", "toggle"]), None);
    let t2 = tg.clone();
    Widget::toggle(&tg, None, move |st| {
        let on = st["airplane"]["on"] == true;
        t2.set(on, "", if on { t("every radio off") } else { "" });
    })
}
