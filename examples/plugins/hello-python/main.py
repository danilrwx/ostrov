#!/usr/bin/env python3
"""ostrov's example plugin: a toggle counting its clicks by a step picked in its menu, with a reset and a level."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ostrov_plugin import Plugin  # noqa: E402

STEPS = [1, 5, 10]
RENAME = 1  # the timer the rename's dialog is asked from

# ostrov's settings schema (docs/plugins.md): on the Settings page as "Hello", the fields in [plugin.hello-python],
# the token in the keyring
SCHEMA = {"sections": [{"title": "Hello", "fields": [
    {"key": "greeting", "title": "Greeting", "type": "string", "default": "Hello"},
    {"key": "token", "title": "API token", "type": "secret", "help": "kept in the keyring",
     "actions": [{"label": "Test", "id": "test-token"}]},
]}]}


class Hello(Plugin):
    count, on, step, level, note, title = 0, False, 0, 0.5, "", ""

    def on_config(self, config):
        self.set_settings_schema(SCHEMA)
        self.kick()

    def state(self):
        return {"count": self.count, "on": self.on}

    def run(self, args, input=None):
        match args:
            case ["count"]:
                return str(self.count)
            case ["set", n]:
                self.count = int(n)
            case ["reset"]:
                self.count = 0
            case ["note"]:
                self.note = (input or "").strip()
            case ["rename"]:
                # asked from a timer: a command answers at once, the dialog waits for the user
                self.set_timer(0, RENAME)
                return "asking"
            case ["action", "test-token"]:  # the token's Test button, its value as input
                if not input:
                    raise ValueError("no token typed")
                return f"a token of {len(input)} characters"
            case _:
                return super().run(args)
        self.kick()
        return str(self.count)

    def on_timer(self, id):
        if id == RENAME:
            t = self.ask(kind="text", icon="document-edit-symbolic", title="Rename the counter",
                         text="Its toggle's title", value=self.title or self.config.get("greeting", "Hello"), ok="Rename")
            if t:
                self.title = t
                self.kick()

    def render(self, widget):
        token = self.secret("token")
        return {
            "type": "toggle", "id": "count", "icon": "face-smile-symbolic",
            "title": self.title or self.config.get("greeting", "Hello"),
            "sub": f"{self.count} clicks", "on": self.on,
            "menu": {"type": "box", "children": [
                {"type": "row", "id": "add", "icon": "list-add-symbolic", "text": f"Add {STEPS[self.step]}"},
                {"type": "row", "id": "reset", "icon": "edit-clear-symbolic", "text": "Reset", "note": str(self.count)},
                {"type": "chips", "id": "step", "options": [str(s) for s in STEPS], "on": self.step},
                {"type": "slider", "id": "level", "icon": "weather-clear-symbolic", "value": self.level},
                {"type": "progress", "value": min(self.count / 50, 1)},
                {"type": "label", "text": self.note or "echo text | ostrov plugin hello-python note"},
                {"type": "label", "class": "dim",
                 "text": "a token is set" if token else "no token: Settings, Hello"},
            ]},
        }

    def on_event(self, widget, node, event, value):
        if node == "count":
            self.on = value == "true"
            self.count += STEPS[self.step]
        elif node == "add":
            self.count += STEPS[self.step]
        elif node == "reset":
            self.count = 0
        elif node == "step":
            self.step = int(value)
        elif node == "level":
            self.level = float(value)
            return  # the slider shows it already
        self.kick()


Hello().main()
