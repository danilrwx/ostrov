#!/bin/sh
# The README's screenshots, made again: ostrov in a Hyprland of its own, nested in the running session's (a window
# shows for a minute) but drawing on a headless output of its own, the only one captured, with a temporary HOME,
# runtime directory and session bus, and made-up state (OSTROV_DEMO=demo.json, a calendar served from 127.0.0.1,
# toasts, a few apps), so nothing of this machine's shows. Needs a built workspace (cargo build --release
# --workspace), Hyprland, dbus-run-session and python3 with PIL; run from a Wayland session; the PNGs land beside
# this script, KEEP=1 leaves the sandbox. CONTRIBUTING.md, "Screenshots".
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin=$(cd "$here/../.." && pwd)/target/release

if [ "${1-}" != inside ]; then
    # the sandbox, its runtime directory in it: TMPDIR short, for the sockets' paths to fit
    sb=$(mktemp -d)
    trap '[ -n "${KEEP-}" ] || rm -rf "$sb"' EXIT
    mkdir -p "$sb/home/.config/ostrov" "$sb/home/Pictures/wallpapers" "$sb/share/applications" \
        "$sb/share/ostrov/plugins" "$sb/www" "$sb/raw"
    mkdir -m 700 "$sb/run"
    # the system's icons and schemas, not its apps; caldav for the calendar
    ln -s /usr/share/icons "$sb/share/icons"
    ln -s /usr/share/glib-2.0 "$sb/share/glib-2.0"
    cp -r "$here/../../plugins/caldav" "$here/../../plugins/night" "$sb/share/ostrov/plugins/"
    for app in "Files:system-file-manager" "Terminal:utilities-terminal" "Text Editor:accessories-text-editor" \
        "Music:multimedia-audio-player" "Web Browser:web-browser" "Calculator:accessories-calculator" \
        "Settings:preferences-system" "Image Viewer:image-viewer" "Mail:mail-client"; do
        printf '[Desktop Entry]\nType=Application\nName=%s\nIcon=%s\nExec=true\n' "${app%%:*}" "${app#*:}" \
            >"$sb/share/applications/$(echo "${app%%:*}" | tr ' A-Z' '-a-z').desktop"
    done
    cat >"$sb/home/.config/ostrov/config.toml" <<'EOF'
[appearance]
theme = "dark"
accent = "#8b7cf6"

[polkit]
agent = false

[plugin.night]
enabled = true

[plugin.caldav]
enabled = true
ics = ["http://127.0.0.1:8737/demo.ics"]
EOF
    # events from an hour from now on, so Coming Up has some whatever the time; a standup every Monday
    at() { date -d "$1" +%Y%m%dT%H%M00; }
    day() { date -d "$1" +%Y%m%d; }
    event() { printf 'BEGIN:VEVENT\nUID:%s\nSUMMARY:%s\n%s\nEND:VEVENT\n' "$1" "$2" "$3"; }
    {
        printf 'BEGIN:VCALENDAR\nVERSION:2.0\n'
        event review "Design review" "DTSTART:$(at "+1 hour")
DTEND:$(at "+2 hours")"
        event standup Standup "DTSTART:$(day "last monday")T100000
DTEND:$(day "last monday")T101500
RRULE:FREQ=WEEKLY;BYDAY=MO"
        event dentist Dentist "DTSTART:$(day "+1 day")T090000
DTEND:$(day "+1 day")T100000"
        event lunch Lunch "DTSTART:$(day "+2 days")T130000
DTEND:$(day "+2 days")T140000
LOCATION:Café"
        event trip "Trip to the sea" "DTSTART;VALUE=DATE:$(day "+3 days")
DTEND;VALUE=DATE:$(day "+5 days")"
        event concert Concert "DTSTART:$(day "+6 days")T200000
DTEND:$(day "+6 days")T223000"
        printf 'END:VCALENDAR\n'
    } >"$sb/www/demo.ics"
    # the player's cover; demo.json's @COVER@
    sed "s|@COVER@|file://$sb/cover.png|" "$here/demo.json" >"$sb/demo.json"
    # the wallpaper: soft light over the dark, drawn here rather than kept in the repository
    python3 - "$sb/home/Pictures/wallpapers/aurora.png" "$sb/cover.png" <<'EOF'
import sys
from PIL import Image, ImageDraw, ImageFilter
W, H = 2560, 1440
img = Image.new("RGB", (W, H), (14, 16, 32))
for (cx, cy), r, col in [((0.15, 0.25), 0.55, (72, 52, 160)), ((0.85, 0.2), 0.5, (20, 110, 150)),
                         ((0.7, 0.9), 0.6, (140, 60, 130)), ((0.2, 0.95), 0.45, (30, 60, 120)),
                         ((0.5, 0.5), 0.35, (50, 40, 110))]:
    m = Image.new("L", (W, H), 0)
    R = r * W / 2
    ImageDraw.Draw(m).ellipse((cx * W - R, cy * H - R, cx * W + R, cy * H + R), fill=200)
    img = Image.composite(Image.new("RGB", (W, H), col), img, m.filter(ImageFilter.GaussianBlur(R * 0.6)))
img = img.filter(ImageFilter.GaussianBlur(40))
img.save(sys.argv[1])
img.crop((900, 200, 1700, 1000)).resize((256, 256)).save(sys.argv[2])
EOF
    # a HiDPI screen, 1280x800 in points: a headless output, the nested window's own turned off once it is there
    cat >"$sb/hypr.conf" <<EOF
monitor = SHOT, 2560x1600@60, 0x0, 2
animations {
    enabled = false
}
cursor {
    inactive_timeout = 0.5
}
misc {
    disable_hyprland_logo = true
    disable_splash_rendering = true
}
exec-once = sh -c 'hyprctl output create headless SHOT; sleep 1; hyprctl keyword monitor WAYLAND-1,disable; hyprctl dispatch focusmonitor SHOT; exec "$here/make.sh" inside'
EOF
    host="${XDG_RUNTIME_DIR:?a Wayland session}/${WAYLAND_DISPLAY:?a Wayland session}"
    env -i PATH="$bin:$PATH" ${LD_LIBRARY_PATH:+LD_LIBRARY_PATH="$LD_LIBRARY_PATH"} HOME="$sb/home" USER=demo \
        LANG=en_US.UTF-8 XDG_RUNTIME_DIR="$sb/run" XDG_DATA_DIRS="$sb/share" OSTROV_APP_ID=dev.ostrov.Screenshots \
        OSTROV_DEMO="$sb/demo.json" WAYLAND_DISPLAY="$host" SB="$sb" \
        dbus-run-session -- Hyprland -c "$sb/hypr.conf" >"$sb/hyprland.log" 2>&1
    # cropped to what each is of, at most 1600 px wide
    python3 - "$sb/raw" "$here" <<'EOF'
import sys
from PIL import Image
raw, out = sys.argv[1:]
# (left, top, right, bottom) on the 2560x1600 screen
crops = {
    "hero": (0, 0, 2560, 1600), "calendar": (340, 0, 2220, 1310), "launcher": (0, 0, 2560, 120),
    "edit": (1440, 0, 2560, 1540), "appearance": (1440, 0, 2560, 1320), "welcome": (680, 300, 1880, 1300),
    "toast": (1760, 0, 2560, 250), "lock": (680, 440, 1880, 1120), "bar": (0, 0, 2560, 760),
    "night": (1720, 0, 2560, 1120), "kit": (1440, 0, 2560, 1600),
}
for name, box in crops.items():
    img = Image.open(f"{raw}/{name}.png").convert("RGB").crop(box)
    img.thumbnail((1600, 1600), Image.LANCZOS)
    img.save(f"{out}/{name}.png", optimize=True)
EOF
    exit
fi

# inside Hyprland: ostrov, then each view opened, captured and closed
raw=$SB/raw
(cd "$SB/www" && exec python3 -m http.server -b 127.0.0.1 8737 >/dev/null 2>&1) &
www=$!
ostrov >"$SB/ostrov.log" 2>&1 &
shell=$!
o() { ostrov "$@" </dev/null >/dev/null; }
# the screen, once it has settled; the PNG is written a moment after the command answers
shot() {
    sleep "${2:-1.5}"
    o capture "$raw/$1.png"
    for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$raw/$1.png" ] && break; sleep 0.5; done
    sleep 0.5
}
sleep 3
o wallpaper set "$HOME/Pictures/wallpapers/aurora.png"
o wallpaper on
o toast "Download finished" "holiday-photos.zip, 48 MB"
o toast "Backup" "Your files are backed up."
sleep 6
o toast "Meeting in 10 minutes" "Design review"
shot toast 1
sleep 6
o menu wifi
shot hero
o menu brightness
shot night
o panel
o calendar
shot calendar
o calendar
o run
shot launcher
o run
o menu edit
shot edit
o panel
o appearance
shot appearance
o kit
shot kit
o panel
o bar edit
shot bar
o bar edit
o welcome
shot welcome
o lock
shot lock 2
kill $shell $www
hyprctl dispatch exit
