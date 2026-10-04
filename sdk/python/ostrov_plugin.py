"""ostrov's plugin SDK for Python: wit/ostrov-plugin.wit over JSON lines on stdin and stdout (docs/plugins.md).

Subclass Plugin, override the exports you need (render above all), call the host's imports, and run main():

    class Hello(Plugin):
        def render(self, widget):
            return {"type": "toggle", "id": "t", "title": "Hello", "on": self.on}

        def on_event(self, widget, node, event, value):
            self.on = value == "true"
            self.kick()

    Hello().main()

The imports that answer (host_run, secret, http_get) wait for their answer, handling ostrov's other messages
meanwhile. Nothing but protocol may be printed on stdout: log with self.log, or print to stderr.
"""

import itertools
import json
import sys

API = 1


class OstrovError(Exception):
    """What went wrong, as ostrov said it."""


class Plugin:
    config = {}

    # the exports, called by ostrov
    def state(self):
        return None

    def run(self, args, input=None):
        """`ostrov plugin <id> ARGS`, input what was piped to it: its output, or OstrovError."""
        raise OstrovError(f"no command {' '.join(args)!r}")

    def render(self, widget):
        return None

    def on_event(self, widget, node, event, value):
        pass

    def on_timer(self, id):
        pass

    def on_config(self, config):
        pass

    def on_state(self, state):
        pass

    # the imports, called on ostrov
    def log(self, msg):
        self._send({"type": "log", "msg": str(msg)})

    def host_run(self, *args):
        """One of ostrov's commands (permission "run"): its output, or OstrovError."""
        r = self._call({"type": "run", "args": [str(a) for a in args]})
        if "err" in r:
            raise OstrovError(r["err"])
        return r.get("ok", "")

    def ask(self, **dialog):
        """A dialog (permission "dialogs"): kind="confirm"|"text"|"secret"|"choice"|"form", title=..., and the
        kind's fields (docs/plugins.md). Its answer, None for a no; a form's as a dict. Blocks till answered, so
        never from run (its caller waits 10 s): set_timer(0, ...) and ask from on_timer."""
        a = self._call({"type": "ask", "dialog": dialog})
        return json.loads(a) if a is not None and dialog.get("kind") == "form" else a

    def secret(self, key):
        """A secret from the keyring (permission "secrets"), None if there is none."""
        return self._call({"type": "secret", "key": key})

    def http_get(self, url):
        """A URL's body as bytes (permission "network"), or OstrovError."""
        r = self._call({"type": "http_get", "url": url})
        if "err" in r:
            raise OstrovError(r["err"])
        return bytes(r["ok"])

    def set_timer(self, ms, id):
        self._send({"type": "set_timer", "ms": int(ms), "id": int(id)})

    def kick(self):
        """The state changed: ostrov calls state and render again."""
        self._send({"type": "kick"})

    def set_settings_schema(self, schema):
        self._send({"type": "set_settings_schema", "json": schema})

    # pushes, a process plugin's alternative to kick
    def push_render(self, widget, tree):
        self._send({"type": "render", "widget": widget, "tree": tree})

    def push_state(self, state):
        self._send({"type": "state", "json": state})

    # the wire
    _calls = itertools.count(1)

    def _send(self, msg):
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()

    def _read(self):
        line = sys.stdin.readline()
        if not line:
            sys.exit(0)  # ostrov is gone
        return json.loads(line)

    def _call(self, msg):
        n = next(self._calls)
        self._send(dict(msg, call=n))
        while True:
            m = self._read()
            if m.get("type") == "return" and m.get("call") == n:
                if "error" in m:
                    raise OstrovError(m["error"])
                return m.get("value")
            self._handle(m)

    def _handle(self, m):
        t, call = m.get("type"), m.get("call")
        try:
            if t == "hello":
                if m.get("api") != API:
                    print(f"ostrov speaks api {m.get('api')}, this SDK {API}", file=sys.stderr)
                return
            if t == "on_config":
                self.config = m.get("json") or {}
                return self.on_config(self.config)
            if t == "on_state":
                return self.on_state(m.get("json"))
            if t == "on_event":
                return self.on_event(m["widget"], m["node"], m["event"], m["value"])
            if t == "on_timer":
                return self.on_timer(m["id"])
            if t == "state":
                value = self.state()
            elif t == "render":
                value = self.render(m["widget"])
            elif t == "run_request":
                try:
                    r = {"ok": str(self.run(m["args"], m.get("input")) or "")}
                except Exception as e:  # its error is the command's
                    r = {"err": str(e)}
                return self._send(dict(r, type="run_result", id=m["id"]))
            else:
                return  # a later ostrov's message
            self._send({"type": "return", "call": call, "value": value})
        except Exception as e:  # a broken handler answers, and the plugin goes on
            print(f"{t}: {e!r}", file=sys.stderr)
            if call is not None and t in ("state", "render"):
                self._send({"type": "return", "call": call, "error": repr(e)})

    def main(self):
        while True:
            self._handle(self._read())
