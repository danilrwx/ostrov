# claude

A launcher mode of ostrov's: `?question` in the launcher (`ostrov run`) is one row, "Ask Claude: question",
opening https://claude.ai/new?q=question in the browser. `ostrov plugin claude ask QUESTION...` does the same
from a script or a key.

    cp -rL examples/plugins/claude ~/.local/share/ostrov/plugins/

and restart ostrov. It needs `python3`, and `xdg-open` for the command; no permissions.

The row carries `"open": URL`, so ostrov opens it itself and the plugin has no `pick`: the smallest launcher mode
there is (see docs/plugins.md, Launcher modes). `examples/plugins/google` is the same for `g words`.
